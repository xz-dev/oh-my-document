//! difftastic adapter: classify a changed file by shelling out to `difft`.
//!
//! EAFP over preflight checks — every text-mode pair is fed to difftastic;
//! whatever it can't classify comes back Unclassified (conservative: stays
//! in the review queue). No extension allowlist, no language gate —
//! difftastic's own Text fallback already covers unknown extensions.
//!
//! Signal: `difft --exit-code <old> <new>` — exit 0 = no syntactic change
//! (CosmeticOnly), 1 = changed (Changed), anything else / spawn failure /
//! output parse failure = Unclassified. `DFT_UNSTABLE=yes` enables the
//! (unstable, evidence-only) JSON output for a second pass that harvests a
//! bounded summary: language, status, chunk count, comment_only, and the
//! first few chunk coordinates. `env_clear()` prevents ambient `DFT_*`
//! variables from silently changing semantics.

use std::io::Write;
use std::process::Command;

use omd::relations::classify::{
    CHUNK_HEAD, Classification, ClassificationEvidence, DiffChunk, DiffClassifier,
};

/// Classify one file pair through the `difft` binary.
pub struct DifftasticClassifier {
    /// Tool identity line captured once at construction (`difft --version`).
    version: String,
}

impl DifftasticClassifier {
    /// Probe the binary once. A missing/failing binary is NOT an error here —
    /// construction still succeeds; each `classify` call then returns
    /// Unclassified honestly (one probe, no per-file retry storms).
    pub fn probe() -> Self {
        let version = run_difft(&["--version"])
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().next().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "difft (unavailable)".into());
        Self { version }
    }

    /// Write both contents to temp files keeping the real extension —
    /// difftastic picks its grammar from the filename.
    fn with_temp_pair<R>(
        &self,
        filename_hint: &str,
        old_bytes: &[u8],
        new_bytes: &[u8],
        f: impl FnOnce(&std::path::Path, &std::path::Path) -> R,
    ) -> Result<R, String> {
        let suffix = std::path::Path::new(filename_hint)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{e}"))
            .unwrap_or_default();
        let mut old_file =
            tempfile::NamedTempFile::with_suffix(&suffix).map_err(|e| e.to_string())?;
        let mut new_file =
            tempfile::NamedTempFile::with_suffix(&suffix).map_err(|e| e.to_string())?;
        old_file.write_all(old_bytes).map_err(|e| e.to_string())?;
        new_file.write_all(new_bytes).map_err(|e| e.to_string())?;
        old_file.flush().map_err(|e| e.to_string())?;
        new_file.flush().map_err(|e| e.to_string())?;
        Ok(f(old_file.path(), new_file.path()))
    }
}

/// Run `difft` with a clean environment — ambient `DFT_*` (exit-code,
/// overrides, limits) would silently change semantics.
fn run_difft(args: &[&str]) -> Result<std::process::Output, std::io::Error> {
    Command::new("difft")
        .args(args)
        .env_clear()
        .env("DFT_UNSTABLE", "yes")
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .stdin(std::process::Stdio::null())
        .output()
}

/// Harvest a bounded evidence summary from `--display json` output.
/// Returns None on any parse trouble — evidence is optional, never fatal.
fn evidence_from_json(stdout: &[u8], tool: &str) -> Option<ClassificationEvidence> {
    let text = std::str::from_utf8(stdout).ok()?;
    let line = text.lines().find(|l| l.trim_start().starts_with('{'))?;
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let language = v
        .get("language")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    let status = v.get("status").and_then(|x| x.as_str()).map(str::to_string);
    let mut chunks_head = Vec::new();
    let mut chunk_count = 0usize;
    let mut comment_only = true;
    let mut saw_chunk = false;
    if let Some(chunks) = v.get("chunks").and_then(|c| c.as_array()) {
        for chunk in chunks {
            for side_name in ["lhs", "rhs"] {
                let Some(side) = chunk.get(side_name) else {
                    continue;
                };
                let line = side
                    .get("line_number")
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0) as usize;
                if let Some(changes) = side.get("changes").and_then(|c| c.as_array()) {
                    for change in changes {
                        saw_chunk = true;
                        chunk_count += 1;
                        let highlight = change
                            .get("highlight")
                            .and_then(|h| h.as_str())
                            .unwrap_or("normal")
                            .to_string();
                        if highlight != "comment" {
                            comment_only = false;
                        }
                        if chunks_head.len() < CHUNK_HEAD {
                            chunks_head.push(DiffChunk {
                                line,
                                side: side_name.to_string(),
                                highlight,
                            });
                        }
                    }
                }
            }
        }
    }
    Some(ClassificationEvidence {
        tool: tool.to_string(),
        language,
        status,
        chunk_count: saw_chunk.then_some(chunk_count),
        comment_only: saw_chunk.then_some(comment_only),
        chunks_head,
        note: None,
    })
}

impl DiffClassifier for DifftasticClassifier {
    fn tool_id(&self) -> String {
        self.version.clone()
    }

    fn classify(&self, filename_hint: &str, old_bytes: &[u8], new_bytes: &[u8]) -> Classification {
        if old_bytes == new_bytes {
            return Classification::CosmeticOnly(ClassificationEvidence {
                tool: self.version.clone(),
                language: None,
                status: Some("unchanged".into()),
                chunk_count: Some(0),
                comment_only: None,
                chunks_head: Vec::new(),
                note: Some("identical bytes".into()),
            });
        }
        // EAFP: feed it to difft; every failure path lands in Unclassified.
        let (exit_code, evidence) =
            match self.with_temp_pair(filename_hint, old_bytes, new_bytes, |old_path, new_path| {
                let old_s = old_path.to_string_lossy().into_owned();
                let new_s = new_path.to_string_lossy().into_owned();
                let decision = run_difft(&["--exit-code", &old_s, &new_s]);
                let json = run_difft(&["--display", "json", &old_s, &new_s]);
                (decision, json)
            }) {
                Err(e) => return Classification::Unclassified(format!("temp files: {e}")),
                Ok((decision, json_out)) => {
                    let evidence = json_out
                        .ok()
                        .and_then(|o| evidence_from_json(&o.stdout, &self.version))
                        .unwrap_or_else(|| ClassificationEvidence {
                            tool: self.version.clone(),
                            language: None,
                            status: None,
                            chunk_count: None,
                            comment_only: None,
                            chunks_head: Vec::new(),
                            note: Some("json_unavailable".into()),
                        });
                    match decision {
                        Err(e) => return Classification::Unclassified(format!("difft: {e}")),
                        Ok(o) => (o.status.code().unwrap_or(-1), evidence),
                    }
                }
            };
        match exit_code {
            0 => Classification::CosmeticOnly(evidence),
            1 => Classification::Changed(evidence),
            code => Classification::Unclassified(format!("difft exit {code}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_bytes_are_cosmetic_without_tool() {
        let c = DifftasticClassifier {
            version: "test".into(),
        };
        assert!(matches!(
            c.classify("a.rs", b"x", b"x"),
            Classification::CosmeticOnly(_)
        ));
    }

    #[test]
    fn whitespace_only_rs_is_cosmetic() {
        let c = DifftasticClassifier::probe();
        match c.classify(
            "a.rs",
            b"fn f() { let x = 1; }\n",
            b"fn f() { let x  =  1; }\n",
        ) {
            Classification::CosmeticOnly(_) => {}
            other => panic!("expected CosmeticOnly, got {other:?}"),
        }
    }

    #[test]
    fn logic_change_is_changed() {
        let c = DifftasticClassifier::probe();
        match c.classify(
            "a.rs",
            b"fn f() { let x = 1; }\n",
            b"fn f() { let x = 2; }\n",
        ) {
            Classification::Changed(e) => {
                assert_eq!(e.language.as_deref(), Some("Rust"));
                assert_eq!(e.status.as_deref(), Some("changed"));
            }
            other => panic!("expected Changed, got {other:?}"),
        }
    }

    #[test]
    fn unknown_extension_still_classified() {
        // EAFP contract: no extension gate — difft's Text fallback decides.
        let c = DifftasticClassifier::probe();
        let r = c.classify("a.unknownext", b"a b c\n", b"a b\n");
        assert!(matches!(r, Classification::Changed(_)));
    }
}
