//! Notes: separate append-only records keyed to commits (E-12.1).
//!
//! A note is `{id, timestamp, commit_id, kind, target_note_id, text}` —
//! independent 128-bit id, never an author/reply-tree/approval field. `patch`
//! and `delete` append a record pointing at the original note id; the
//! original is never overwritten in place. Revision order follows the
//! publication sequence, not wall-clock — a clock that goes backwards must
//! not reorder revisions. Notes never change a commit's id/reason/dirty.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Independent 128-bit id (hex).
    pub id: String,
    /// RFC3339 timestamp — display only, never revision order.
    pub timestamp: String,
    /// The commit this note annotates.
    pub commit_id: String,
    /// "add" | "patch" | "delete".
    pub kind: String,
    /// For patch/delete: the original note id this revises ("" for add).
    #[serde(default)]
    pub target_note_id: String,
    pub text: String,
    /// Publication sequence for revision order (not the clock).
    pub seq: u64,
}

/// Append a note record into `notes/<id>.toml`. Returns the note id.
pub fn append(
    root: &std::path::Path,
    note: &Note,
) -> Result<String, std::io::Error> {
    let path = root.join(format!("notes/{}.toml", note.id));
    let txt = toml::to_string(note).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, txt)?;
    Ok(note.id.clone())
}

/// List notes for `commit_id` in publication order (seq, not timestamp).
pub fn list_for(root: &std::path::Path, commit_id: &str) -> Vec<Note> {
    let mut out: Vec<Note> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root.join("notes")) {
        for e in rd.flatten() {
            if let Ok(txt) = std::fs::read_to_string(e.path()) {
                if let Ok(n) = toml::from_str::<Note>(&txt) {
                    if n.commit_id == commit_id {
                        out.push(n);
                    }
                }
            }
        }
    }
    out.sort_by_key(|n| n.seq);
    out
}

/// Reverse lookup: notes pointing at `note_id` (its patch/delete history).
pub fn revisions_of(root: &std::path::Path, note_id: &str) -> Vec<Note> {
    let mut out: Vec<Note> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root.join("notes")) {
        for e in rd.flatten() {
            if let Ok(txt) = std::fs::read_to_string(e.path()) {
                if let Ok(n) = toml::from_str::<Note>(&txt) {
                    if n.target_note_id == note_id {
                        out.push(n);
                    }
                }
            }
        }
    }
    out.sort_by_key(|n| n.seq);
    out
}
