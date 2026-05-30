//! KiCad → bhdl footprint **translator**.
//!
//! Architectural decision (2026-05-30): the toolchain imports **bhdl
//! only** — no runtime KiCad dependency (symbols or footprints). KiCad is
//! a one-time *translation source*, not a runtime input. So this module's
//! headline job is `translate_kicad_mod()`: parse a `.kicad_mod` and emit
//! a bhdl `footprint { }` declaration (the `footprint_spec_v0.md` §3.5
//! form), which becomes the canonical, source-controlled artifact. The
//! runtime then imports that `.bhdl`, never the `.kicad_mod`.
//!
//! Forward dependency: the emitted text is parsed by the synth-owned
//! `footprint` grammar (spec steps 4–6), not yet built — so round-trip
//! (translate → parse → consume) lights up when the grammar lands. The
//! translator is built against the spec §3.5 grammar in the meantime.
//!
//! The in-memory `import_kicad_mod()` / `convert()` path (→
//! `ComponentFootprint`) is retained as the translator's geometry
//! front-end and for tests; it is **not** the canonical runtime path.

use bhdl_components::kicad::parser::{
    KiCadFootprint, KiCadFootprintGraphic, KiCadFootprintParser, KiCadPad,
};
use bhdl_components::types::component::{
    ComponentFootprint, FootprintPad, PadShape, PadType,
};

/// Result of importing a `.kicad_mod`: the P&R-consumable footprint plus
/// the explicit courtyard extent (from the `F.CrtYd` graphics), which the
/// `ComponentFootprint` itself has nowhere to store yet.
#[derive(Debug, Clone)]
pub struct ImportedFootprint {
    pub footprint: ComponentFootprint,
    /// Courtyard rectangle extent (w, h) in mm from `F.CrtYd`, if present.
    /// The manufacturable keepout — preferred over the board-wide IPC
    /// excess when known.
    pub courtyard: Option<(f64, f64)>,
}

/// Parse a `.kicad_mod` file body and convert to an `ImportedFootprint`.
pub fn import_kicad_mod(content: &str) -> Result<ImportedFootprint, String> {
    let parser = KiCadFootprintParser::new();
    let kfp = parser
        .parse_footprint(content)
        .map_err(|e| format!("kicad_mod parse error: {e:?}"))?;
    Ok(convert(&kfp))
}

/// Convert a parsed `KiCadFootprint` to the P&R `ComponentFootprint` +
/// extracted courtyard. Pads keep their KiCad designators and
/// coordinates (KiCad's +Y-down frame matches the P&R convention).
pub fn convert(kfp: &KiCadFootprint) -> ImportedFootprint {
    let pads: Vec<FootprintPad> = kfp.pads.iter().map(convert_pad).collect();

    let courtyard = graphic_bbox(&kfp.graphics, "F.CrtYd")
        .or_else(|| graphic_bbox(&kfp.graphics, "B.CrtYd"));
    // Body extent: prefer the fab outline, else the courtyard, else the
    // pad bounding box.
    let (body_w, body_h) = graphic_bbox(&kfp.graphics, "F.Fab")
        .or(courtyard)
        .or_else(|| pad_bbox(&pads))
        .unwrap_or((0.0, 0.0));

    let footprint = ComponentFootprint {
        footprint_name: kfp.name.clone(),
        svg_data: String::new(),
        pad_count: pads.len() as u32,
        body_width: body_w,
        body_height: body_h,
        pitch: infer_pitch(&pads),
        pads,
    };

    ImportedFootprint { footprint, courtyard }
}

fn convert_pad(p: &KiCadPad) -> FootprintPad {
    FootprintPad {
        pad_number: p.number.clone(),
        x_position: p.x,
        y_position: p.y,
        width: p.size_x,
        height: p.size_y,
        shape: map_shape(&p.shape),
        drill_diameter: p.drill,
        pad_type: map_pad_type(&p.pad_type),
    }
    // NOTE: pad rotation (p.rotation) is not yet applied — most pads are
    // axis-aligned; rotated-oval orientation is a v1 refinement.
}

fn map_shape(s: &str) -> PadShape {
    match s {
        "circle" => PadShape::Circle,
        "oval" => PadShape::Oval,
        "roundrect" => PadShape::RoundedRectangle,
        // "rect", "trapezoid", "custom" → rectangle (closest supported)
        _ => PadShape::Rectangle,
    }
}

fn map_pad_type(t: &str) -> PadType {
    match t {
        "smd" => PadType::SMD,
        "np_thru_hole" => PadType::NPTH,
        // "thru_hole", "connect", anything else → through-hole
        _ => PadType::ThroughHole,
    }
}

/// Axis-aligned bounding-box extent (w, h) of all line/arc/poly graphics
/// on a given layer. Used for courtyard and fab-body extraction.
fn graphic_bbox(graphics: &[KiCadFootprintGraphic], layer: &str) -> Option<(f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;

    let mut acc = |x: f64, y: f64| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };

    for g in graphics {
        match g {
            KiCadFootprintGraphic::Line { start_x, start_y, end_x, end_y, layer: l, .. }
                if l == layer =>
            {
                acc(*start_x, *start_y);
                acc(*end_x, *end_y);
                any = true;
            }
            KiCadFootprintGraphic::Circle { center_x, center_y, end_x, end_y, layer: l, .. }
                if l == layer =>
            {
                let r = ((end_x - center_x).powi(2) + (end_y - center_y).powi(2)).sqrt();
                acc(center_x - r, center_y - r);
                acc(center_x + r, center_y + r);
                any = true;
            }
            KiCadFootprintGraphic::Arc { start_x, start_y, end_x, end_y, layer: l, .. }
                if l == layer =>
            {
                acc(*start_x, *start_y);
                acc(*end_x, *end_y);
                any = true;
            }
            _ => {}
        }
    }

    if any {
        Some((max_x - min_x, max_y - min_y))
    } else {
        None
    }
}

fn pad_bbox(pads: &[FootprintPad]) -> Option<(f64, f64)> {
    if pads.is_empty() {
        return None;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for p in pads {
        min_x = min_x.min(p.x_position - p.width / 2.0);
        min_y = min_y.min(p.y_position - p.height / 2.0);
        max_x = max_x.max(p.x_position + p.width / 2.0);
        max_y = max_y.max(p.y_position + p.height / 2.0);
    }
    Some((max_x - min_x, max_y - min_y))
}

/// Infer pin pitch from the two closest distinct pad centers (for
/// metadata; not load-bearing).
fn infer_pitch(pads: &[FootprintPad]) -> Option<f64> {
    let mut min_d = f64::INFINITY;
    for i in 0..pads.len() {
        for j in (i + 1)..pads.len() {
            let d = ((pads[i].x_position - pads[j].x_position).powi(2)
                + (pads[i].y_position - pads[j].y_position).powi(2))
            .sqrt();
            if d > 1e-6 {
                min_d = min_d.min(d);
            }
        }
    }
    if min_d.is_finite() {
        Some((min_d * 1000.0).round() / 1000.0)
    } else {
        None
    }
}

// ── Translator: .kicad_mod → bhdl `footprint { }` text ───────────────────

/// Translate a `.kicad_mod` file body into a bhdl `footprint { }`
/// declaration (the canonical, runtime-consumed form). This is the
/// one-time KiCad→bhdl conversion; the toolchain imports the result, not
/// the `.kicad_mod`.
pub fn translate_kicad_mod(content: &str) -> Result<String, String> {
    let parser = KiCadFootprintParser::new();
    let kfp = parser
        .parse_footprint(content)
        .map_err(|e| format!("kicad_mod parse error: {e:?}"))?;
    Ok(kicad_to_bhdl(&kfp))
}

/// Serialize a parsed `KiCadFootprint` to a bhdl `footprint { }`
/// declaration per `footprint_spec_v0.md` §3.5. Pads are keyed by
/// designator with **no pin names** (the entity binds those on import);
/// courtyard + body are emitted; silk/fab graphics are preserved as
/// opaque blocks for round-trip fidelity.
pub fn kicad_to_bhdl(kfp: &KiCadFootprint) -> String {
    let name = sanitize_ident(&kfp.name);
    let mut s = String::new();
    s.push_str(&format!("// Translated from KiCad footprint \"{}\".\n", kfp.name));
    s.push_str(&format!("footprint {name} {{\n"));

    // Pads — geometry only, keyed by designator.
    for p in &kfp.pads {
        let kind = match p.pad_type.as_str() {
            "smd" => "smd",
            "np_thru_hole" => "npth",
            _ => "tht",
        };
        let shape = match p.shape.as_str() {
            "circle" => "circle",
            "oval" => "oval",
            "roundrect" => "roundrect",
            _ => "rect",
        };
        let layer = pad_layer(&p.layers);
        s.push_str(&format!(
            "    pad \"{}\" {kind} {shape} at ({}, {}) size ({}, {})",
            p.number,
            fmt(p.x),
            fmt(p.y),
            fmt(p.size_x),
            fmt(p.size_y),
        ));
        if let Some(l) = layer {
            s.push_str(&format!(" layer {l}"));
        }
        if let Some(d) = p.drill {
            s.push_str(&format!(" drill {}", fmt(d)));
        }
        s.push_str(";\n");
    }

    // Courtyard + body extents.
    if let Some((w, h)) = graphic_bbox(&kfp.graphics, "F.CrtYd")
        .or_else(|| graphic_bbox(&kfp.graphics, "B.CrtYd"))
    {
        s.push_str(&format!("    courtyard rect ({}, {});\n", fmt(w), fmt(h)));
    }
    if let Some((w, h)) = graphic_bbox(&kfp.graphics, "F.Fab") {
        s.push_str(&format!("    body rect ({}, {});\n", fmt(w), fmt(h)));
    }

    // Opaque pass-through: silk + fab graphics, preserved verbatim as
    // line records so export can round-trip. The toolchain does not
    // interpret these.
    emit_opaque(&mut s, &kfp.graphics, "F.SilkS", "silk");
    emit_opaque(&mut s, &kfp.graphics, "F.Fab", "fab");

    s.push_str("}\n");
    s
}

/// Map a KiCad pad layer list to a bhdl layer keyword. Through-hole pads
/// (`*.Cu`) span all layers → no keyword (default). SMD on the front →
/// `top`, back → `bottom`.
fn pad_layer(layers: &[String]) -> Option<&'static str> {
    let has = |n: &str| layers.iter().any(|l| l == n);
    if has("*.Cu") {
        None // through-hole spans all
    } else if has("F.Cu") {
        Some("top")
    } else if has("B.Cu") {
        Some("bottom")
    } else {
        None
    }
}

fn emit_opaque(s: &mut String, graphics: &[KiCadFootprintGraphic], layer: &str, block: &str) {
    let lines: Vec<&KiCadFootprintGraphic> = graphics
        .iter()
        .filter(|g| matches!(g,
            KiCadFootprintGraphic::Line { layer: l, .. } if l == layer))
        .collect();
    if lines.is_empty() {
        return;
    }
    s.push_str(&format!("    {block} {{\n"));
    for g in lines {
        if let KiCadFootprintGraphic::Line { start_x, start_y, end_x, end_y, .. } = g {
            s.push_str(&format!(
                "        line ({}, {}) -> ({}, {});\n",
                fmt(*start_x), fmt(*start_y), fmt(*end_x), fmt(*end_y)
            ));
        }
    }
    s.push_str("    }\n");
}

/// Format a float compactly: trim trailing zeros, keep up to 4 decimals.
fn fmt(v: f64) -> String {
    let r = (v * 10000.0).round() / 10000.0;
    let mut s = format!("{r}");
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

/// Turn a KiCad footprint name into a valid bhdl identifier.
fn sanitize_ident(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if out.chars().next().map_or(true, |c| c.is_ascii_digit()) {
        out.insert(0, 'F');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal real-shaped .kicad_mod: an SMD 0603 resistor with a
    // courtyard and fab outline. Coordinates in mm, KiCad v6 syntax.
    const R0603: &str = r#"
(footprint "R_0603_1608Metric" (layer "F.Cu")
  (descr "Resistor SMD 0603")
  (attr smd)
  (fp_line (start -1.48 0.73) (end 1.48 0.73) (layer "F.CrtYd"))
  (fp_line (start 1.48 0.73) (end 1.48 -0.73) (layer "F.CrtYd"))
  (fp_line (start 1.48 -0.73) (end -1.48 -0.73) (layer "F.CrtYd"))
  (fp_line (start -1.48 -0.73) (end -1.48 0.73) (layer "F.CrtYd"))
  (fp_line (start -0.8 0.4) (end 0.8 0.4) (layer "F.Fab"))
  (fp_line (start 0.8 0.4) (end 0.8 -0.4) (layer "F.Fab"))
  (fp_line (start 0.8 -0.4) (end -0.8 -0.4) (layer "F.Fab"))
  (fp_line (start -0.8 -0.4) (end -0.8 0.4) (layer "F.Fab"))
  (pad "1" smd roundrect (at -0.8 0) (size 0.9 0.95) (layers "F.Cu" "F.Paste" "F.Mask"))
  (pad "2" smd roundrect (at 0.8 0) (size 0.9 0.95) (layers "F.Cu" "F.Paste" "F.Mask"))
)
"#;

    #[test]
    fn imports_0603_pads_and_courtyard() {
        let imported = import_kicad_mod(R0603).expect("parse");
        let fp = &imported.footprint;

        assert_eq!(fp.pad_count, 2);
        assert_eq!(fp.pads.len(), 2);

        let p1 = fp.pads.iter().find(|p| p.pad_number == "1").unwrap();
        let p2 = fp.pads.iter().find(|p| p.pad_number == "2").unwrap();
        // Real KiCad pad coordinates preserved (not IPC-regenerated).
        assert!((p1.x_position - (-0.8)).abs() < 1e-6);
        assert!((p2.x_position - 0.8).abs() < 1e-6);
        assert!((p1.width - 0.9).abs() < 1e-6);
        assert!(matches!(p1.shape, PadShape::RoundedRectangle));
        assert!(matches!(p1.pad_type, PadType::SMD));

        // Courtyard extracted from F.CrtYd: 2.96 × 1.46 mm.
        let (cw, ch) = imported.courtyard.expect("courtyard");
        assert!((cw - 2.96).abs() < 0.01, "courtyard w {cw}");
        assert!((ch - 1.46).abs() < 0.01, "courtyard h {ch}");

        // Pitch inferred = 1.6mm (pad-to-pad).
        assert!((fp.pitch.unwrap() - 1.6).abs() < 0.01);
    }

    #[test]
    fn translates_0603_to_bhdl_footprint() {
        let bhdl = translate_kicad_mod(R0603).expect("translate");
        // Well-formed declaration with a sanitized identifier.
        assert!(bhdl.contains("footprint R_0603_1608Metric {"), "got:\n{bhdl}");
        // Pads by designator, geometry only — no pin names (no `->` on pads).
        assert!(bhdl.contains(r#"pad "1" smd roundrect at (-0.8, 0) size (0.9, 0.95) layer top;"#), "got:\n{bhdl}");
        assert!(bhdl.contains(r#"pad "2" smd roundrect at (0.8, 0) size (0.9, 0.95) layer top;"#));
        // Courtyard + body extents emitted.
        assert!(bhdl.contains("courtyard rect (2.96, 1.46);"), "got:\n{bhdl}");
        assert!(bhdl.contains("body rect (1.6, 0.8);"), "got:\n{bhdl}");
        // Fab graphics preserved as an opaque block.
        assert!(bhdl.contains("fab {"), "got:\n{bhdl}");
        // No pin-name binding leaked into the footprint (that lives on the
        // entity). Pads must not contain '->' on their own lines.
        for line in bhdl.lines().filter(|l| l.trim_start().starts_with("pad ")) {
            assert!(!line.contains("->"), "pad line must carry no pin binding: {line}");
        }
    }

    #[test]
    fn translates_through_hole_with_drill() {
        let dip = r#"
(footprint "DIP-8_W7.62mm" (layer "F.Cu")
  (pad "1" thru_hole rect (at 0 0) (size 1.6 1.6) (drill 0.8) (layers "*.Cu" "*.Mask"))
)
"#;
        let bhdl = translate_kicad_mod(dip).expect("translate");
        // Through-hole: no layer keyword (spans all), drill present.
        assert!(bhdl.contains(r#"pad "1" tht rect at (0, 0) size (1.6, 1.6) drill 0.8;"#), "got:\n{bhdl}");
    }

    #[test]
    fn through_hole_pads_carry_drill() {
        let dip = r#"
(footprint "DIP-8" (layer "F.Cu")
  (pad "1" thru_hole rect (at 0 0) (size 1.6 1.6) (drill 0.8) (layers "*.Cu"))
  (pad "2" thru_hole oval (at 0 2.54) (size 1.6 1.6) (drill 0.8) (layers "*.Cu"))
)
"#;
        let imported = import_kicad_mod(dip).expect("parse");
        let p1 = &imported.footprint.pads[0];
        assert!(matches!(p1.pad_type, PadType::ThroughHole));
        assert!((p1.drill_diameter.unwrap() - 0.8).abs() < 1e-6);
        assert!(matches!(p1.shape, PadShape::Rectangle));
    }
}
