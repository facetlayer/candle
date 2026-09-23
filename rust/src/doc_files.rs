//! Documentation files for the `list-docs` and `get-doc` commands.
//!
//! Ported from `src/docFiles/DocFilesHelper.ts`. Unlike the Node version — which reads markdown
//! files from the installed package directory at runtime — the Rust binary is relocatable, so the
//! docs (the repo `docs/` directory plus the top-level `README.md`) are embedded at compile time.
//!
//! Only markdown files directly inside `docs/` are user-facing. Subdirectories such as `docs/dev/`
//! hold developer docs for people working on Candle itself, and are left out of both commands.

use include_dir::{include_dir, Dir};

static DOCS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../docs");
const README: &str = include_str!("../../README.md");

/// The README has no frontmatter, so `list-docs` uses this description for it.
const README_DESCRIPTION: &str = "Full reference for every command (the project README)";

/// Metadata about a doc file, pulled from its frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocInfo {
    pub name: String,
    pub description: String,
    pub filename: String,
}

/// A resolved doc: its filename, where it lives in the repo, and its content
/// with the frontmatter stripped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocContent {
    pub filename: String,
    /// Repo-relative path of the source file, e.g. `docs/project-setup.md`.
    pub source_path: String,
    pub content: String,
}

/// Why `get_doc` failed to resolve a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocLookupError {
    NotFound,
}

/// All user-facing doc files as `(filename, raw_content)`, sorted by filename for stable output,
/// with `README.md` included last (matching the Node config which appends it as an extra file).
/// `Dir::files()` only yields the top level of `docs/`, so `docs/dev/` is never included.
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

/// Repo-relative path of an embedded doc. `README.md` is the repo's top-level
/// README; everything else comes from `docs/`.
fn source_path(filename: &str) -> String {
    if filename == "README.md" {
        filename.to_string()
    } else {
        format!("docs/{filename}")
    }
}

/// List all docs with metadata from frontmatter (`name`/`description`), falling back to the filename
/// stem for `name`.
pub fn list_docs() -> Vec<DocInfo> {
    all_docs()
        .into_iter()
        .map(|(filename, raw)| {
            let (name, description, _) = parse_frontmatter(raw);
            let description = description.unwrap_or_else(|| {
                if filename == "README.md" {
                    README_DESCRIPTION.to_string()
                } else {
                    String::new()
                }
            });
            DocInfo {
                name: name.unwrap_or_else(|| stem(&filename).to_string()),
                description,
                filename,
            }
        })
        .collect()
}

/// Resolve a doc by name: the key `list-docs` shows (its frontmatter `name`,
/// or the filename without `.md`), or the filename itself. Matching is exact
/// apart from letter case; there is no prefix or substring matching, so
/// `get-doc project` does not pick up `project-setup`.
pub fn get_doc(name: &str) -> Result<DocContent, DocLookupError> {
    let wanted = name.trim().to_lowercase();
    let wanted_stem = stem(&wanted).to_string();
    if wanted_stem.is_empty() {
        return Err(DocLookupError::NotFound);
    }

    all_docs()
        .into_iter()
        .find(|(filename, raw)| {
            let (front_name, _, _) = parse_frontmatter(raw);
            stem(filename).to_lowercase() == wanted_stem
                || front_name.is_some_and(|n| n.to_lowercase() == wanted_stem)
        })
        .map(|(filename, raw)| {
            let (_, _, content) = parse_frontmatter(raw);
            DocContent {
                source_path: source_path(&filename),
                filename,
                content,
            }
        })
        .ok_or(DocLookupError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_embedded_docs() {
        let docs = list_docs();
        assert!(docs.iter().any(|d| d.filename == "project-setup.md"));
        assert!(docs.iter().any(|d| d.filename == "transient-processes.md"));
        // README is appended as an extra file.
        assert!(docs.iter().any(|d| d.filename == "README.md"));
    }

    #[test]
    fn excludes_dev_docs() {
        assert!(DOCS_DIR.get_file("dev/testing-strategy.md").is_some());
        assert!(!list_docs().iter().any(|d| d.name == "testing-strategy"));
        assert_eq!(get_doc("testing-strategy"), Err(DocLookupError::NotFound));
    }

    #[test]
    fn get_doc_exact_names() {
        let d = get_doc("project-setup").unwrap();
        assert_eq!(d.filename, "project-setup.md");
        assert_eq!(d.source_path, "docs/project-setup.md");
        assert!(d.content.contains("Project Setup"));

        // The filename form works too.
        assert_eq!(
            get_doc("project-setup.md").unwrap().filename,
            "project-setup.md"
        );

        let t = get_doc("transient-processes").unwrap();
        assert!(t.content.contains("Transient"));
    }

    #[test]
    fn get_doc_does_not_prefix_match() {
        assert_eq!(get_doc("start"), Err(DocLookupError::NotFound));
        assert_eq!(get_doc("project"), Err(DocLookupError::NotFound));
        assert_eq!(get_doc(""), Err(DocLookupError::NotFound));
    }

    #[test]
    fn get_doc_strips_frontmatter() {
        let d = get_doc("agents-intro").unwrap();
        assert!(!d.content.starts_with("---"));
        assert!(!d.content.contains("description:"));
    }

    #[test]
    fn readme_source_is_repo_root() {
        let d = get_doc("README").unwrap();
        assert_eq!(d.source_path, "README.md");
    }

    #[test]
    fn every_listed_key_resolves() {
        for doc in list_docs() {
            assert_eq!(get_doc(&doc.name).unwrap().filename, doc.filename);
        }
    }

    #[test]
    fn get_doc_not_found() {
        assert_eq!(
            get_doc("nonexistent-doc-xyz"),
            Err(DocLookupError::NotFound)
        );
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
