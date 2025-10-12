//! Markdown formatter utilities

/// Markdown formatter (currently generates directly, could be refactored to use this)
pub struct MarkdownFormatter;

impl MarkdownFormatter {
    /// Create markdown table header
    pub fn table_header(columns: &[&str]) -> String {
        let mut output = String::new();
        output.push_str("| ");
        output.push_str(&columns.join(" | "));
        output.push_str(" |\n");

        output.push_str("|");
        for _ in columns {
            output.push_str("--------|");
        }
        output.push('\n');

        output
    }

    /// Create markdown table row
    pub fn table_row(values: &[&str]) -> String {
        format!("| {} |\n", values.join(" | "))
    }

    /// Create markdown heading
    pub fn heading(level: usize, text: &str) -> String {
        format!("{} {}\n\n", "#".repeat(level), text)
    }

    /// Create markdown list item
    pub fn list_item(text: &str) -> String {
        format!("- {}\n", text)
    }

    /// Create markdown code block
    pub fn code_block(language: &str, code: &str) -> String {
        format!("```{}\n{}\n```\n", language, code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_header() {
        let header = MarkdownFormatter::table_header(&["Col1", "Col2", "Col3"]);
        assert!(header.contains("| Col1 | Col2 | Col3 |"));
        assert!(header.contains("|--------|"));
    }

    #[test]
    fn test_heading() {
        assert_eq!(MarkdownFormatter::heading(1, "Title"), "# Title\n\n");
        assert_eq!(MarkdownFormatter::heading(2, "Subtitle"), "## Subtitle\n\n");
    }
}
