//! Standalone HTML generation — bundles viewer assets into a self-contained HTML file.
//!
//! ELK.js is loaded as a **separate `<script>` tag** (not concatenated with our code)
//! for EPL-2.0 license compliance.

use crate::types::SchematicData;

const VIEWER_HTML: &str = include_str!("../viewer/index.html");
const VIEWER_JS: &str = include_str!("../viewer/schematic.js");
const ELK_JS: &str = include_str!("../viewer/elk.bundled.js");

/// Generate a standalone HTML file containing the schematic viewer and embedded data.
///
/// The output is a self-contained HTML file that can be opened in any modern browser.
/// It includes ELK.js for layout and the Canvas-based renderer.
pub fn generate_standalone_html(data: &SchematicData) -> String {
    let json = serde_json::to_string(data).expect("SchematicData serialization should not fail");

    // ELK is in its own <script> tag, separate from our MIT-licensed code
    VIEWER_HTML
        .replace("{{ELK_SCRIPT}}", ELK_JS)
        .replace("{{SCHEMATIC_SCRIPT}}", VIEWER_JS)
        .replace("{{SCHEMATIC_DATA}}", &json)
}
