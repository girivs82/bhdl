/// Generate SVG grid background pattern
pub fn generate_grid_background() -> String {
    "<defs>\n<pattern id=\"grid\" width=\"20\" height=\"20\" patternUnits=\"userSpaceOnUse\">\n<path d=\"M 20 0 L 0 0 0 20\" fill=\"none\" stroke=\"#e0e0e0\" stroke-width=\"0.5\"/>\n</pattern>\n</defs>\n<rect width=\"100%\" height=\"100%\" fill=\"url(#grid)\"/>".to_string()
} 