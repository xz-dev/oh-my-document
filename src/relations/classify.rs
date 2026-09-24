//! Structural classification of a changed file: did the change alter the
//! syntax tree, or only its surface?
//!
//! The classifier answers a single question for a (old_bytes, new_bytes)
//! pair: CosmeticOnly (structure unchanged — cosmetic/formatting only),
//! Changed (structure changed), or Unclassified (the classifier could not
//! decide — missing tool, parse failure, byte mode, etc.). Unclassified is
//! deliberately conservative: the caller MUST keep the range in the review
//! queue, never silently drop it.
//!
//! The trait is a library-layer seam — implementations live at the CLI
//! layer (e.g. a difftastic adapter shelling out to `difft`). No heavy
//! dependency is pulled into the core.

/// One chunk of a structural diff, summarised for evidence/reporting.
///
/// `line` is the 0-based line on the changed side; `side` is `"lhs"` (old)
/// or `"rhs"` (new); `highlight` is the classifier's syntactic category for
/// the changed token (`"comment"`, `"string"`, `"keyword"`, `"normal"`,
/// `"delimiter"`, …) when reported.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffChunk {
    pub line: usize,
    pub side: String,
    pub highlight: String,
}

/// A bounded, evidence-grade summary of what an external classifier saw.
/// Carried on each bucket entry (dirty and cosmetic) so the human/Agent can
/// judge without re-running the tool. Deliberately does NOT carry raw
/// content strings — line/column coordinates only.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClassificationEvidence {
    /// Tool identity string (e.g. `"difft 0.69.0"` or `"difft"`).
    pub tool: String,
    /// Detected language label, e.g. `"Rust"`, `"Text"`,
    /// `"Text (1 Rust parse error, exceeded DFT_PARSE_ERROR_LIMIT)"`.
    pub language: Option<String>,
    /// Classifier's own status word (`"changed"`, `"unchanged"`, …).
    pub status: Option<String>,
    /// Total changed chunks reported.
    pub chunk_count: Option<usize>,
    /// True when every reported chunk's highlight is `comment` — the
    /// human/agent can see at a glance "only comments moved".
    pub comment_only: Option<bool>,
    /// First few chunks (≤ `CHUNK_HEAD`) — bounded head, never the full list.
    pub chunks_head: Vec<DiffChunk>,
    /// Why no structured evidence was captured (`"json_unavailable"`,
    /// `"tool_failed"`, `"byte_mode"`, …). Present only when the above
    /// fields could not be produced.
    pub note: Option<String>,
}

/// Bound on `chunks_head` — evidence is a pointer, not a payload.
pub const CHUNK_HEAD: usize = 3;

/// Result of one classification attempt.
#[derive(Debug, Clone)]
pub enum Classification {
    /// Structural tree unchanged — cosmetic/formatting only.
    CosmeticOnly(ClassificationEvidence),
    /// Structural tree changed.
    Changed(ClassificationEvidence),
    /// The classifier could not decide. The caller MUST treat this as
    /// "still dirty" — conservative, never silently cosmetic.
    Unclassified(String),
}

/// Structural classifier for one file pair. `classify` receives the
/// complete old and new source bytes plus a filename hint (for syntax
/// selection) and returns a `Classification`.
///
/// Implementations MUST be honest: any failure to run the tool, parse its
/// output, or reach a decision returns `Unclassified` — never guessed.
pub trait DiffClassifier: Send + Sync {
    /// Tool identity for evidence, e.g. `"difft 0.69.0"`. Cheap to call.
    fn tool_id(&self) -> String;

    /// Classify one (old, new) content pair.
    fn classify(&self, filename_hint: &str, old_bytes: &[u8], new_bytes: &[u8]) -> Classification;
}

/// A no-op classifier for tests and for callers that never got the flag.
/// Always returns `Unclassified` — honest, conservative, zero work.
pub struct NoopClassifier;

impl DiffClassifier for NoopClassifier {
    fn tool_id(&self) -> String {
        "none".into()
    }

    fn classify(&self, _f: &str, _o: &[u8], _n: &[u8]) -> Classification {
        Classification::Unclassified("no classifier configured".into())
    }
}
