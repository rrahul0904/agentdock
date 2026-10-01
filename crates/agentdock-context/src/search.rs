use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use thiserror::Error;

pub const SEARCH_SCHEMA_VERSION: u8 = 1;
const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_INDEX_BYTES: usize = 64 * 1024 * 1024;
const MAX_QUERY_BYTES: usize = 512;

const EXCLUDED_COMPONENTS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cache",
    ".idea",
    ".vscode",
    ".next",
    ".turbo",
    ".venv",
    "venv",
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
    "vendor",
];

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("path is not safe to index: {0}")]
    UnsafePath(String),
    #[error("duplicate document path: {0}")]
    DuplicatePath(String),
    #[error("document exceeds the per-document byte limit: {path}")]
    DocumentTooLarge { path: String },
    #[error("index batch exceeds the total byte limit")]
    IndexTooLarge,
    #[error("query must not be empty")]
    EmptyQuery,
    #[error("query exceeds the byte limit")]
    QueryTooLarge,
    #[error("invalid line range")]
    InvalidLineRange,
    #[error("document not found: {0}")]
    DocumentNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexDocument {
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchOptions {
    pub max_hits: usize,
    pub max_total_bytes: usize,
    pub max_snippet_bytes: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            max_hits: 20,
            max_total_bytes: 8 * 1024,
            max_snippet_bytes: 512,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SearchVerdict {
    Strong,
    Weak,
    NoAnswer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchHit {
    pub path: String,
    pub line: usize,
    pub score: u32,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResponse {
    pub schema_version: u8,
    pub query: String,
    pub verdict: SearchVerdict,
    pub hits: Vec<SearchHit>,
    pub indexed_documents: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadResponse {
    pub schema_version: u8,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutlineItem {
    pub line: usize,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutlineResponse {
    pub schema_version: u8,
    pub path: String,
    pub items: Vec<OutlineItem>,
    pub truncated: bool,
}

#[derive(Debug)]
struct Candidate {
    hit: SearchHit,
    exact_phrase: bool,
    all_tokens: bool,
}

pub struct LexicalIndex {
    conn: Connection,
}

impl LexicalIndex {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SearchError> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, SearchError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, SearchError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS search_metadata (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS search_documents (
                 path TEXT PRIMARY KEY,
                 text TEXT NOT NULL,
                 byte_len INTEGER NOT NULL,
                 line_count INTEGER NOT NULL
             );
             INSERT INTO search_metadata(key, value)
             VALUES('schema_version', '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
        )?;

        Ok(Self { conn })
    }

    pub fn replace_documents(&mut self, documents: &[IndexDocument]) -> Result<(), SearchError> {
        let mut seen = HashSet::new();
        let mut total_bytes = 0usize;
        let mut ordered = documents.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| left.path.cmp(&right.path));

        for document in &ordered {
            if !is_indexable_path(&document.path) {
                return Err(SearchError::UnsafePath(document.path.clone()));
            }
            if !seen.insert(document.path.clone()) {
                return Err(SearchError::DuplicatePath(document.path.clone()));
            }
            if document.text.len() > MAX_DOCUMENT_BYTES {
                return Err(SearchError::DocumentTooLarge {
                    path: document.path.clone(),
                });
            }
            total_bytes = total_bytes.saturating_add(document.text.len());
            if total_bytes > MAX_INDEX_BYTES {
                return Err(SearchError::IndexTooLarge);
            }
        }

        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM search_documents", [])?;

        for document in ordered {
            let line_count = document.text.lines().count();
            tx.execute(
                "INSERT INTO search_documents(path, text, byte_len, line_count)
                 VALUES(?1, ?2, ?3, ?4)",
                params![
                    document.path,
                    document.text,
                    document.text.len() as i64,
                    line_count as i64
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn document_count(&self) -> Result<usize, SearchError> {
        let count = self
            .conn
            .query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
                row.get::<_, i64>(0)
            })?;
        Ok(count.max(0) as usize)
    }

    pub fn search(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<SearchResponse, SearchError> {
        let normalized_query = query.trim();
        if normalized_query.is_empty() {
            return Err(SearchError::EmptyQuery);
        }
        if normalized_query.len() > MAX_QUERY_BYTES {
            return Err(SearchError::QueryTooLarge);
        }

        let tokens = tokenize(normalized_query);
        if tokens.is_empty() {
            return Err(SearchError::EmptyQuery);
        }

        let query_lower = normalized_query.to_lowercase();
        let indexed_documents = self.document_count()?;
        let mut stmt = self
            .conn
            .prepare("SELECT path, text FROM search_documents ORDER BY path ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut candidates = Vec::new();

        for row in rows {
            let (path, text) = row?;
            let path_lower = path.to_lowercase();

            for (offset, line) in text.lines().enumerate() {
                let line_lower = line.to_lowercase();
                let exact_phrase = line_lower.contains(&query_lower);
                let token_matches = tokens
                    .iter()
                    .map(|token| line_lower.matches(token).count())
                    .sum::<usize>();
                let all_tokens = tokens.iter().all(|token| line_lower.contains(token));
                let path_matches = tokens
                    .iter()
                    .filter(|token| path_lower.contains(token.as_str()))
                    .count();

                if !exact_phrase && token_matches == 0 && path_matches == 0 {
                    continue;
                }

                let mut score = 0u32;
                if exact_phrase {
                    score = score.saturating_add(1_000);
                }
                if all_tokens {
                    score = score.saturating_add(500);
                }
                score = score.saturating_add((token_matches.min(100) as u32) * 20);
                score = score.saturating_add((path_matches.min(20) as u32) * 5);

                candidates.push(Candidate {
                    hit: SearchHit {
                        path: path.clone(),
                        line: offset + 1,
                        score,
                        snippet: truncate_utf8(line.trim(), options.max_snippet_bytes),
                    },
                    exact_phrase,
                    all_tokens,
                });
            }
        }

        candidates.sort_by(|left, right| {
            right
                .hit
                .score
                .cmp(&left.hit.score)
                .then_with(|| left.hit.path.cmp(&right.hit.path))
                .then_with(|| left.hit.line.cmp(&right.hit.line))
        });

        let max_hits = options.max_hits.min(200);
        let max_total_bytes = options.max_total_bytes.min(256 * 1024);
        let mut used_bytes = 0usize;
        let mut hits = Vec::new();
        let mut truncated = false;
        let mut strongest = SearchVerdict::NoAnswer;

        for candidate in candidates {
            if hits.len() >= max_hits {
                truncated = true;
                break;
            }

            let estimated_bytes = candidate.hit.path.len() + candidate.hit.snippet.len() + 32;
            if used_bytes.saturating_add(estimated_bytes) > max_total_bytes {
                truncated = true;
                continue;
            }

            if hits.is_empty() {
                strongest = if candidate.exact_phrase || candidate.all_tokens {
                    SearchVerdict::Strong
                } else {
                    SearchVerdict::Weak
                };
            }

            used_bytes = used_bytes.saturating_add(estimated_bytes);
            hits.push(candidate.hit);
        }

        if hits.is_empty() {
            strongest = SearchVerdict::NoAnswer;
        }

        Ok(SearchResponse {
            schema_version: SEARCH_SCHEMA_VERSION,
            query: normalized_query.to_string(),
            verdict: strongest,
            hits,
            indexed_documents,
            truncated,
        })
    }

    pub fn read(
        &self,
        path: &str,
        start_line: usize,
        end_line: usize,
        max_bytes: usize,
    ) -> Result<ReadResponse, SearchError> {
        if !is_indexable_path(path) {
            return Err(SearchError::UnsafePath(path.to_string()));
        }
        if start_line == 0 || end_line < start_line {
            return Err(SearchError::InvalidLineRange);
        }

        let text = self
            .conn
            .query_row(
                "SELECT text FROM search_documents WHERE path = ?1",
                params![path],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| SearchError::DocumentNotFound(path.to_string()))?;

        let selected = text
            .lines()
            .enumerate()
            .filter(|(offset, _)| {
                let line = offset + 1;
                line >= start_line && line <= end_line
            })
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
            .join("\n");

        let bounded_max = max_bytes.min(256 * 1024);
        let truncated = selected.len() > bounded_max;
        let text = truncate_utf8(&selected, bounded_max);
        let actual_lines = text.lines().count();
        let actual_end = if actual_lines == 0 {
            start_line.saturating_sub(1)
        } else {
            start_line + actual_lines - 1
        };

        Ok(ReadResponse {
            schema_version: SEARCH_SCHEMA_VERSION,
            path: path.to_string(),
            start_line,
            end_line: actual_end.min(end_line),
            text,
            truncated,
        })
    }

    pub fn outline(&self, path: &str, max_items: usize) -> Result<OutlineResponse, SearchError> {
        if !is_indexable_path(path) {
            return Err(SearchError::UnsafePath(path.to_string()));
        }

        let text = self
            .conn
            .query_row(
                "SELECT text FROM search_documents WHERE path = ?1",
                params![path],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| SearchError::DocumentNotFound(path.to_string()))?;

        let limit = max_items.min(500);
        let mut items = Vec::new();
        let mut truncated = false;

        for (offset, line) in text.lines().enumerate() {
            let Some((kind, label)) = outline_label(line) else {
                continue;
            };

            if items.len() >= limit {
                truncated = true;
                break;
            }

            items.push(OutlineItem {
                line: offset + 1,
                kind: kind.to_string(),
                label: truncate_utf8(label, 512),
            });
        }

        Ok(OutlineResponse {
            schema_version: SEARCH_SCHEMA_VERSION,
            path: path.to_string(),
            items,
            truncated,
        })
    }
}

pub fn is_indexable_path(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() || Path::new(trimmed).is_absolute() {
        return false;
    }

    let normalized = trimmed.replace('\\', "/");
    let components = normalized.split('/').collect::<Vec<_>>();
    if components
        .iter()
        .any(|component| component.is_empty() || *component == ".." || component.contains(':'))
    {
        return false;
    }

    if components.iter().any(|component| {
        EXCLUDED_COMPONENTS
            .iter()
            .any(|item| component.eq_ignore_ascii_case(item))
    }) {
        return false;
    }

    let Some(file_name) = components.last() else {
        return false;
    };

    !is_sensitive_file_name(file_name)
}

fn is_sensitive_file_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower == ".env"
        || lower.starts_with(".env.")
        || matches!(
            lower.as_str(),
            ".npmrc"
                | ".pypirc"
                | ".netrc"
                | "credentials"
                | "credentials.json"
                | "secrets.json"
                | "id_rsa"
                | "id_ed25519"
        )
        || lower.contains("credential")
        || lower.contains("secret")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".p12")
        || lower.ends_with(".pfx")
}

fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn outline_label(line: &str) -> Option<(&'static str, &str)> {
    let trimmed = line.trim();
    let prefixes = [
        ("function", "pub fn "),
        ("function", "fn "),
        ("function", "async fn "),
        ("type", "pub struct "),
        ("type", "struct "),
        ("type", "pub enum "),
        ("type", "enum "),
        ("trait", "pub trait "),
        ("trait", "trait "),
        ("implementation", "impl "),
        ("class", "class "),
        ("function", "def "),
        ("function", "function "),
        ("type", "interface "),
        ("type", "type "),
    ];

    prefixes
        .iter()
        .find_map(|(kind, prefix)| trimmed.strip_prefix(prefix).map(|label| (*kind, label)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_documents() -> Vec<IndexDocument> {
        vec![
            IndexDocument {
                path: "src/context.rs".into(),
                text: "pub struct ContextCapsule {\n    budget: usize,\n}\n\npub fn render_context() {\n    let capsule = ContextCapsule { budget: 512 };\n}\n".into(),
            },
            IndexDocument {
                path: "src/runtime.rs".into(),
                text: "pub fn start_service() {\n    // runtime port reservation\n}\n".into(),
            },
        ]
    }

    #[test]
    fn persistent_search_is_deterministic_and_quality_labeled() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "agentdock-context-search-{}-{nonce}.sqlite",
            std::process::id()
        ));

        {
            let mut index = LexicalIndex::open(&path).expect("open");
            index
                .replace_documents(&fixture_documents())
                .expect("index documents");

            let first = index
                .search("ContextCapsule", SearchOptions::default())
                .expect("first search");
            let second = index
                .search("ContextCapsule", SearchOptions::default())
                .expect("second search");

            assert_eq!(first, second);
            assert_eq!(first.verdict, SearchVerdict::Strong);
            assert_eq!(first.hits.first().expect("hit").path, "src/context.rs");
        }

        let reopened = LexicalIndex::open(&path).expect("reopen");
        let result = reopened
            .search("runtime port", SearchOptions::default())
            .expect("search after reopen");
        assert_eq!(result.indexed_documents, 2);
        assert_eq!(result.verdict, SearchVerdict::Strong);

        drop(reopened);
        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn unsafe_paths_and_duplicates_are_rejected() {
        let mut index = LexicalIndex::in_memory().expect("index");

        for path in [
            ".env",
            "src/secrets.json",
            "node_modules/pkg/index.js",
            "../outside.rs",
            "/absolute/path.rs",
            "C:/absolute/windows.rs",
        ] {
            let error = index
                .replace_documents(&[IndexDocument {
                    path: path.into(),
                    text: "secret".into(),
                }])
                .expect_err("unsafe path should fail");
            assert!(matches!(error, SearchError::UnsafePath(_)));
        }

        let duplicate = IndexDocument {
            path: "src/lib.rs".into(),
            text: "one".into(),
        };
        let error = index
            .replace_documents(&[duplicate.clone(), duplicate])
            .expect_err("duplicate should fail");
        assert!(matches!(error, SearchError::DuplicatePath(_)));
    }

    #[test]
    fn no_answer_is_explicit_and_output_is_bounded() {
        let mut index = LexicalIndex::in_memory().expect("index");
        index
            .replace_documents(&fixture_documents())
            .expect("index documents");

        let missing = index
            .search("definitely_missing_symbol", SearchOptions::default())
            .expect("search");
        assert_eq!(missing.verdict, SearchVerdict::NoAnswer);
        assert!(missing.hits.is_empty());

        let bounded = index
            .search(
                "context",
                SearchOptions {
                    max_hits: 10,
                    max_total_bytes: 80,
                    max_snippet_bytes: 24,
                },
            )
            .expect("bounded search");
        let bytes = bounded
            .hits
            .iter()
            .map(|hit| hit.path.len() + hit.snippet.len() + 32)
            .sum::<usize>();
        assert!(bytes <= 80);
    }

    #[test]
    fn read_and_outline_are_bounded_and_path_scoped() {
        let mut index = LexicalIndex::in_memory().expect("index");
        index
            .replace_documents(&fixture_documents())
            .expect("index documents");

        let read = index.read("src/context.rs", 1, 3, 64).expect("read");
        assert!(read.text.contains("ContextCapsule"));
        assert!(read.end_line <= 3);

        let outline = index.outline("src/context.rs", 10).expect("outline");
        assert!(outline
            .items
            .iter()
            .any(|item| item.label.starts_with("ContextCapsule")));
        assert!(outline
            .items
            .iter()
            .any(|item| item.label.starts_with("render_context")));
    }
}
