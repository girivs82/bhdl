//! Document Link support - provides clickable links for imports and file references

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, ImportStmt};
use std::path::{Path, PathBuf};

/// Provide document links for the document
pub fn provide_document_link(
    text: &str,
    document_uri: &Url,
) -> Option<Vec<DocumentLink>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;

    let mut links = Vec::new();

    // Find all import statements
    for import in source_file.imports() {
        if let Some(link) = create_link_from_import(&import, text, document_uri) {
            links.push(link);
        }
    }

    if links.is_empty() {
        None
    } else {
        Some(links)
    }
}

/// Create a document link from an import statement
fn create_link_from_import(
    import: &ImportStmt,
    _text: &str,
    document_uri: &Url,
) -> Option<DocumentLink> {
    // Get the import path (returns String without quotes)
    let path_str = import.path()?;

    // Get the range of the import in the document
    // We'll use the full import statement range for now
    let text_range = import.syntax().text_range();
    let range = text_range_to_lsp_range(&text_range);

    // Resolve the path relative to the current document
    let target_uri = resolve_import_path(document_uri, &path_str)?;

    Some(DocumentLink {
        range,
        target: Some(target_uri),
        tooltip: Some(format!("Open {}", path_str)),
        data: None,
    })
}

/// Resolve an import path relative to the current document
fn resolve_import_path(document_uri: &Url, import_path: &str) -> Option<Url> {
    // Get the directory of the current document
    let document_path = document_uri.to_file_path().ok()?;
    let document_dir = document_path.parent()?;

    // Resolve the import path relative to the document directory
    let resolved_path = document_dir.join(import_path);

    // Normalize the path (resolve .. and .)
    let canonical_path = normalize_path(&resolved_path);

    // Convert back to URL
    Url::from_file_path(canonical_path).ok()
}

/// Normalize a path by resolving . and .. components
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {
                // Skip current directory components
            }
            _ => {
                components.push(component);
            }
        }
    }

    components.iter().collect()
}

/// Convert rowan TextRange to LSP Range
fn text_range_to_lsp_range(text_range: &rowan::TextRange) -> Range {
    let start: usize = text_range.start().into();
    let end: usize = text_range.end().into();

    Range {
        start: Position {
            line: 0,
            character: start as u32,
        },
        end: Position {
            line: 0,
            character: end as u32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_link_simple_import() {
        let text = r#"
import { LED } from "components/led.bhdl";

board TestBoard {
    power VCC = 5V;
}
"#;

        let uri = Url::parse("file:///project/main.bhdl").unwrap();
        let result = provide_document_link(text, &uri);

        assert!(result.is_some());
        let links = result.unwrap();
        assert_eq!(links.len(), 1);

        let link = &links[0];
        assert!(link.target.is_some());
        assert!(link.tooltip.is_some());
        assert!(link.tooltip.as_ref().unwrap().contains("led.bhdl"));
    }

    #[test]
    fn test_document_link_multiple_imports() {
        let text = r#"
import { LED } from "components/led.bhdl";
import { Resistor } from "components/resistor.bhdl";
import { Regulator } from "modules/regulator.bhdl";

board TestBoard {
    power VCC = 5V;
}
"#;

        let uri = Url::parse("file:///project/main.bhdl").unwrap();
        let result = provide_document_link(text, &uri);

        assert!(result.is_some());
        let links = result.unwrap();
        assert_eq!(links.len(), 3);
    }

    #[test]
    fn test_document_link_relative_path() {
        let text = r#"
import { Component } from "../stdlib/components.bhdl";

board TestBoard {
    power VCC = 5V;
}
"#;

        let uri = Url::parse("file:///project/boards/main.bhdl").unwrap();
        let result = provide_document_link(text, &uri);

        assert!(result.is_some());
        let links = result.unwrap();
        assert_eq!(links.len(), 1);

        let link = &links[0];
        assert!(link.target.is_some());

        // Target should resolve to /project/stdlib/components.bhdl
        let target_path = link.target.as_ref().unwrap().to_file_path().unwrap();
        assert!(target_path.to_string_lossy().contains("stdlib"));
        assert!(target_path.to_string_lossy().contains("components.bhdl"));
    }

    #[test]
    fn test_document_link_no_imports() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
    ground GND;
}
"#;

        let uri = Url::parse("file:///project/main.bhdl").unwrap();
        let result = provide_document_link(text, &uri);

        assert!(result.is_none());
    }

    #[test]
    fn test_normalize_path() {
        let path = PathBuf::from("/project/boards/../stdlib/./components.bhdl");
        let normalized = normalize_path(&path);

        let path_str = normalized.to_string_lossy();
        assert!(!path_str.contains(".."));
        assert!(!path_str.contains("/./"));
        assert!(path_str.contains("stdlib"));
        assert!(path_str.contains("components.bhdl"));
    }

    #[test]
    fn test_resolve_import_path() {
        let doc_uri = Url::parse("file:///project/boards/main.bhdl").unwrap();
        let import_path = "../stdlib/components.bhdl";

        let result = resolve_import_path(&doc_uri, import_path);
        assert!(result.is_some());

        let resolved = result.unwrap();
        let path = resolved.to_file_path().unwrap();
        assert!(path.to_string_lossy().contains("stdlib"));
        assert!(path.to_string_lossy().contains("components.bhdl"));
    }
}
