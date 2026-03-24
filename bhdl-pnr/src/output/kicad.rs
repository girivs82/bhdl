//! KiCad .kicad_pcb export.
//!
//! Generates S-expression format compatible with KiCad 7+.

use crate::types::*;

/// Export board and routes to KiCad PCB format.
pub fn export_kicad_pcb(board: &Board, routes: &[Route]) -> String {
    let mut out = String::new();

    // Header
    out.push_str("(kicad_pcb (version 20230517) (generator \"bhdl-pnr\")\n");
    out.push_str("  (general (thickness 1.6))\n");
    out.push('\n');

    // Page and layers
    let w = board.config.outline.width();
    let h = board.config.outline.height();
    out.push_str(&format!("  (page \"User\" {w} {h})\n"));
    out.push('\n');

    // Layer definitions
    out.push_str("  (layers\n");
    for layer in &board.layer_stack.layers {
        let kicad_type = match layer.kind {
            LayerKind::Signal => "signal",
            LayerKind::Ground | LayerKind::Power | LayerKind::Mixed => "power",
        };
        out.push_str(&format!(
            "    ({} \"{}\" {})\n",
            layer.id, layer.name, kicad_type
        ));
    }
    out.push_str("  )\n\n");

    // Board outline
    match &board.config.outline {
        BoardOutline::Rectangle { width_mm, height_mm } => {
            out.push_str(&format!(
                "  (gr_rect (start 0 0) (end {} {}) (layer \"Edge.Cuts\") (stroke (width 0.05)))\n",
                width_mm, height_mm
            ));
        }
        BoardOutline::Polygon(pts) => {
            out.push_str("  (gr_poly (pts\n");
            for (x, y) in pts {
                out.push_str(&format!("    (xy {} {})\n", x, y));
            }
            out.push_str("  ) (layer \"Edge.Cuts\") (stroke (width 0.05)))\n");
        }
        BoardOutline::AutoSize => {}
    }
    out.push('\n');

    // Mounting holes
    for hole in &board.config.mounting_holes {
        out.push_str(&format!(
            "  (footprint \"MountingHole:MountingHole_{:.1}mm\" (at {} {})\n",
            hole.drill_mm, hole.x_mm, hole.y_mm
        ));
        out.push_str(&format!(
            "    (pad \"\" np_thru_hole circle (at 0 0) (size {} {}) (drill {}))\n",
            hole.drill_mm + 0.5,
            hole.drill_mm + 0.5,
            hole.drill_mm
        ));
        out.push_str("  )\n");
    }
    out.push('\n');

    // Components
    for comp in &board.components {
        let layer_name = match comp.side {
            BoardSide::Top => "F.Cu",
            BoardSide::Bottom => "B.Cu",
        };
        let rot_deg = comp.theta.to_degrees();

        out.push_str(&format!(
            "  (footprint \"{}\" (at {} {} {:.1}) (layer \"{}\")\n",
            comp.package, comp.x, comp.y, rot_deg, layer_name
        ));
        out.push_str(&format!(
            "    (property \"Reference\" \"{}\")\n",
            comp.refdes
        ));

        // Pads
        for pin in &comp.pins {
            out.push_str(&format!(
                "    (pad \"{}\" smd rect (at {} {}) (size 0.5 0.5) (layers \"{}\"))\n",
                pin.name, pin.dx, pin.dy, layer_name
            ));
        }

        out.push_str("  )\n");
    }
    out.push('\n');

    // Routes
    for route in routes {
        for seg in &route.segments {
            let layer_name = board
                .layer_stack
                .layers
                .get(seg.layer)
                .map(|l| l.name.as_str())
                .unwrap_or("F.Cu");
            out.push_str(&format!(
                "  (segment (start {} {}) (end {} {}) (width {}) (layer \"{}\") (net 0))\n",
                seg.start.0, seg.start.1, seg.end.0, seg.end.1, seg.width_mm, layer_name
            ));
        }
        for via in &route.vias {
            let from_name = board
                .layer_stack
                .layers
                .get(via.from_layer)
                .map(|l| l.name.as_str())
                .unwrap_or("F.Cu");
            let to_name = board
                .layer_stack
                .layers
                .get(via.to_layer)
                .map(|l| l.name.as_str())
                .unwrap_or("B.Cu");
            out.push_str(&format!(
                "  (via (at {} {}) (size {}) (drill {}) (layers \"{}\" \"{}\") (net 0))\n",
                via.x,
                via.y,
                board.layer_stack.via.pad_mm,
                board.layer_stack.via.drill_mm,
                from_name,
                to_name
            ));
        }
    }

    out.push_str(")\n");
    out
}
