//! Documentation files for the `list-docs` and `get-doc` commands.
//!
//! Ported from `src/docFiles/DocFilesHelper.ts`. Unlike the Node version — which reads markdown
//! files from the installed package directory at runtime — the Rust binary is relocatable, so the
//! docs (the repo `docs/` directory plus the top-level `README.md`) are embedded at compile time.

use include_dir::{include_dir, Dir};

static DOCS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../docs");
const README: &str = include_str!("../../README.md");

/// Metadata about a doc file, pulled from its frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocInfo {
    pub name: String,
    pub description: String,
    pub filename: String,
}

/// A resolved doc: its filename and full raw content (frontmatter included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocContent {
    pub filename: String,
    pub raw_content: String,
}

/// Why `get_doc` failed to resolve a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocLookupError {
    NotFound,
    Ambiguous(Vec<String>),
}

/// All embedded doc files as `(filename, raw_content)`, sorted by filename for stable output, with
/// `README.md` included last (matching the Node config which appends it as an extra file).
fn all_docs() -> Vec<(String, &'static str)> {
    let mut docs: Vec<(String, &'static str)> = DOCS_DIR
        .files()
        .filter(|f| f.path().extension().map(|e| e == "md").unwrap_or(false))
        .map(|f| {
            (
                f.path().file_name().unwrap().to_string_lossy().into_owned(),
                f.contents_utf8().unwrap_or(""),
            )
        })
        .collect();
    docs.sort_by(|a, b| a.0.cmp(&b.0));
    docs.push(("README.md".to_string(), README));
    docs
}

/// Parse YAML-ish frontmatter delimited by `---`. Only simple `key: value` lines are read; returns
/// `(name, description, content)` where content is the body with the frontmatter stripped.
fn parse_frontmatter(text: &str) -> (Option<String>, Option<String>, String) {
    let normalized = text.replace("\r\n", "\n");
    if let Some(rest) = normalized.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let block = &rest[..end];
            let content = &rest[end + "\n---\n".len()..];
            let mut name = None;
            let mut description = None;
            for line in block.split('\n') {
                if let Some(idx) = line.find(':') {
                    let key = line[..idx].trim();
                    let value = line[idx + 1..].trim().to_string();
                    match key {
                        "name" => name = Some(value),
                        "description" => description = Some(value),
                        _ => {}
                    }
                }
            }
            return (name, description, content.trim().to_string());
        }
    }
    (None, None, text.to_string())
}

fn stem(filename: &str) -> &str {
    filename.strip_suffix(".md").unwrap_or(filename)
}

/// List all docs with metadata from frontmatter (`name`/`description`), falling back to the filename
/// stem for `name`.
pub fn list_docs() -> Vec<DocInfo> {
    all_docs()
        .into_iter()
        .map(|(filename, raw)| {
            let (name, description, _) = parse_frontmatter(raw);
            DocInfo {
                name: name.unwrap_or_else(|| stem(&filename).to_string()),
                description: description.unwrap_or_default(),
                filename,
            }
        })
        .collect()
}

/// Resolve a doc by name. Exact `<name>.md` filename wins; otherwise a case-insensitive substring
/// match against filename or frontmatter name. Zero matches → `NotFound`; multiple → `Ambiguous`.
pub fn get_doc(name: &str) -> Result<DocContent, DocLookupError> {
    let base = name.strip_suffix(".md").unwrap_or(name);
    let target_filename = format!("{base}.md");

    let docs = all_docs();
    if let Some((filename, raw)) = docs.iter().find(|(f, _)| f == &target_filename) {
        return Ok(DocContent {
            filename: filename.clone(),
            raw_content: raw.to_string(),
        });
    }

    let lower = base.to_lowercase();
    let infos = list_docs();
    let matches: Vec<&DocInfo> = infos
        .iter()
        .filter(|d| {
            d.filename.to_lowercase().contains(&lower) || d.name.to_lowercase().contains(&lower)
        })
        .collect();

    match matches.len() {
        0 => Err(DocLookupError::NotFound),
        1 => {
            let filename = matches[0].filename.clone();
            let raw = docs.iter().find(|(f, _)| f == &filename).unwrap().1;
            Ok(DocContent {
                filename,
                raw_content: raw.to_string(),
            })
        }
        _ => Err(DocLookupError::Ambiguous(
            matches.iter().map(|d| d.filename.clone()).collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_embedded_docs() {
        let docs = list_docs();
        assert!(docs.iter().any(|d| d.filename == "getting-started.md"));
        assert!(docs.iter().any(|d| d.filename == "transient-processes.md"));
        // README is appended as an extra file.
        assert!(docs.iter().any(|d| d.filename == "README.md"));
    }

    #[test]
    fn get_doc_exact_and_partial() {
        let d = get_doc("getting-started").unwrap();
        assert_eq!(d.filename, "getting-started.md");
        assert!(d.raw_content.contains("Getting Started"));

        let t = get_doc("transient-processes").unwrap();
        assert!(t.raw_content.contains("Transient"));
    }

    #[test]
    fn get_doc_not_found() {
        assert_eq!(get_doc("nonexistent-doc-xyz"), Err(DocLookupError::NotFound));
    }

    #[test]
    fn frontmatter_parsed() {
        let (name, desc, content) =
            parse_frontmatter("---\nname: foo\ndescription: bar\n---\n# Title\n");
        assert_eq!(name.as_deref(), Some("foo"));
        assert_eq!(desc.as_deref(), Some("bar"));
        assert_eq!(content, "# Title");
    }
}
