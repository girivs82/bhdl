//! DXF mechanical parity gate.
//!
//! The `.bhdl` layout block is the AUTHORITATIVE mechanical contract;
//! the MCAD system's DXF is an EXTERNAL CHECKER the produced board must
//! satisfy — the same doctrine as the KiCad DRC oracle. `mech_check
//! "file.dxf";` compares the board's outline vertices and mounting
//! holes against the DXF's LWPOLYLINE/CIRCLE entities; drift between
//! MCAD and ECAD becomes a BUILD FAILURE instead of a fab-day surprise.
//!
//! Coordinate frames: MCAD DXFs are commonly Y-up while board coords
//! are Y-down; both the direct and Y-mirrored mappings are tried and
//! the matching one is reported.

use anyhow::{bail, Context, Result};

pub struct Dxf {
    pub polylines: Vec<Vec<(f64, f64)>>,
    pub circles: Vec<(f64, f64, f64)>, // (cx, cy, radius)
}

/// Minimal ASCII DXF reader: group-code/value pairs, ENTITIES section,
/// LWPOLYLINE (codes 10/20 per vertex) and CIRCLE (10/20/40).
pub fn parse_dxf(path: &std::path::Path) -> Result<Dxf> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("mech_check: cannot read {}", path.display()))?;
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    let mut polylines = Vec::new();
    let mut circles = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        let code = lines[i];
        let val = lines[i + 1];
        if code == "0" && val == "LWPOLYLINE" {
            let mut pts: Vec<(f64, f64)> = Vec::new();
            let mut x: Option<f64> = None;
            i += 2;
            while i + 1 < lines.len() && lines[i] != "0" {
                match lines[i] {
                    "10" => x = lines[i + 1].parse().ok(),
                    "20" => {
                        if let (Some(px), Ok(py)) = (x.take(), lines[i + 1].parse()) {
                            pts.push((px, py));
                        }
                    }
                    _ => {}
                }
                i += 2;
            }
            if pts.len() >= 3 {
                polylines.push(pts);
            }
            continue;
        }
        if code == "0" && val == "CIRCLE" {
            let (mut cx, mut cy, mut r) = (None, None, None);
            i += 2;
            while i + 1 < lines.len() && lines[i] != "0" {
                match lines[i] {
                    "10" => cx = lines[i + 1].parse().ok(),
                    "20" => cy = lines[i + 1].parse().ok(),
                    "40" => r = lines[i + 1].parse().ok(),
                    _ => {}
                }
                i += 2;
            }
            if let (Some(cx), Some(cy), Some(r)) = (cx, cy, r) {
                circles.push((cx, cy, r));
            }
            continue;
        }
        i += 2;
    }
    Ok(Dxf { polylines, circles })
}

/// Point-set match with tolerance: every expected point has a candidate
/// within tol and the counts agree. Order/starting-vertex insensitive.
fn points_match(expected: &[(f64, f64)], got: &[(f64, f64)], tol: f64) -> bool {
    expected.len() == got.len()
        && expected.iter().all(|&(ex, ey)| {
            got.iter().any(|&(gx, gy)| (ex - gx).hypot(ey - gy) <= tol)
        })
}

pub fn check_parity(
    outline_pts: &[(f64, f64)],
    holes: &[(f64, f64, f64)], // (x, y, drill)
    cutouts: &[(f64, f64, f64, f64)],
    dxf: &Dxf,
    board_h: f64,
) -> Result<()> {
    const TOL: f64 = 0.05;
    let mirror = |pts: &[(f64, f64)]| -> Vec<(f64, f64)> {
        pts.iter().map(|&(x, y)| (x, board_h - y)).collect()
    };

    // Outline: the LARGEST-area DXF polyline is the board edge (the
    // rest are interior apertures); match direct or Y-mirrored.
    let area = |pts: &[(f64, f64)]| -> f64 {
        let n = pts.len();
        (0..n)
            .map(|i| {
                let (ax, ay) = pts[i];
                let (bx, by) = pts[(i + 1) % n];
                ax * by - bx * ay
            })
            .sum::<f64>()
            .abs()
            / 2.0
    };
    let outline_idx = (0..dxf.polylines.len())
        .max_by(|&a, &b| {
            area(&dxf.polylines[a])
                .partial_cmp(&area(&dxf.polylines[b]))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    let Some(outline_idx) = outline_idx else {
        bail!("mech_check FAILED: DXF contains no closed polyline");
    };
    let frame = if points_match(outline_pts, &dxf.polylines[outline_idx], TOL) {
        "direct"
    } else if points_match(&mirror(outline_pts), &dxf.polylines[outline_idx], TOL) {
        "y-mirrored"
    } else {
        bail!(
            "mech_check FAILED: board outline ({} vertices) does not match \
             the DXF's largest polyline (tried direct and y-mirrored frames, \
             tol {TOL}mm)",
            outline_pts.len()
        )
    };

    // Cutouts: every declared aperture must match a remaining DXF
    // polyline in the SAME frame — and every remaining DXF polyline
    // must be a declared aperture (an undeclared hole in the MCAD
    // model is exactly the drift this gate exists to catch).
    let mut claimed: Vec<bool> = dxf.polylines.iter().map(|_| false).collect();
    claimed[outline_idx] = true;
    for &(x0, y0, x1, y1) in cutouts {
        let corners = vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        let corners = if frame == "y-mirrored" {
            mirror(&corners)
        } else {
            corners
        };
        let hit = dxf.polylines.iter().enumerate().find_map(|(k, pl)| {
            (!claimed[k] && points_match(&corners, pl, TOL)).then_some(k)
        });
        match hit {
            Some(k) => claimed[k] = true,
            None => bail!(
                "mech_check FAILED: declared cutout ({x0},{y0})-({x1},{y1}) \
                 has no matching DXF polyline ({frame} frame, tol {TOL}mm)"
            ),
        }
    }
    if let Some(k) = claimed.iter().position(|c| !c) {
        let pl = &dxf.polylines[k];
        bail!(
            "mech_check FAILED: DXF contains an aperture not declared in the \
             .bhdl ({} vertices near ({:.1},{:.1})) — declare it with a \
             `cutout` statement or remove it from the MCAD model",
            pl.len(),
            pl[0].0,
            pl[0].1
        );
    }

    // Holes: every declared hole must have a DXF circle at its position
    // with radius = drill/2, in the SAME frame the outline matched.
    for &(hx, hy, drill) in holes {
        let (ex, ey) = if frame == "y-mirrored" {
            (hx, board_h - hy)
        } else {
            (hx, hy)
        };
        let found = dxf.circles.iter().any(|&(cx, cy, r)| {
            (ex - cx).hypot(ey - cy) <= TOL && (r - drill / 2.0).abs() <= TOL
        });
        if !found {
            bail!(
                "mech_check FAILED: mounting hole at ({hx}, {hy}) drill {drill} \
                 has no matching DXF circle ({} frame, tol {TOL}mm)",
                frame
            );
        }
    }
    println!(
        "  ✓ Mech parity: outline {} vertices ✓, holes {}/{} ✓, cutouts {}/{} ✓ ({} frame)",
        outline_pts.len(),
        holes.len(),
        holes.len(),
        cutouts.len(),
        cutouts.len(),
        frame
    );
    Ok(())
}

/// Format a mm coordinate for HDL output: 3 decimals, trailing zeros
/// trimmed.
fn fmt_mm(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_string() } else { s.to_string() }
}

fn polygon_area(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    a.abs() / 2.0
}

/// Transcribe a MCAD DXF into layout-block mechanical statements: the
/// one-time onboarding path. After pasting the output the .bhdl is
/// authoritative and the emitted `mech_check` line keeps both sides
/// honest on every subsequent build.
pub fn render_import(dxf: &Dxf, dxf_name: &str, flip_y: bool) -> Result<String> {
    if dxf.polylines.is_empty() {
        bail!("mech-import: no closed LWPOLYLINE found — nothing to use as a board outline");
    }
    // Largest-area polyline is the outline; anything else is likely an
    // interior feature we don't model yet (cutouts are a queued arc).
    let outline_idx = (0..dxf.polylines.len())
        .max_by(|&a, &b| {
            polygon_area(&dxf.polylines[a])
                .partial_cmp(&polygon_area(&dxf.polylines[b]))
                .unwrap()
        })
        .unwrap();
    let mut pts = dxf.polylines[outline_idx].clone();
    let mut holes: Vec<(f64, f64, f64)> =
        dxf.circles.iter().map(|&(x, y, r)| (x, y, 2.0 * r)).collect();

    if flip_y {
        for p in &mut pts {
            p.1 = -p.1;
        }
        for h in &mut holes {
            h.1 = -h.1;
        }
    }
    // Normalize: board frame has the outline's min corner at (0,0).
    let minx = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let miny = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    for p in &mut pts {
        p.0 -= minx;
        p.1 -= miny;
    }
    for h in &mut holes {
        h.0 -= minx;
        h.1 -= miny;
    }

    let mut out = String::new();
    if minx.abs() > 1e-9 || miny.abs() > 1e-9 {
        out.push_str(&format!(
            "// translated by ({}, {}) so the outline's min corner sits at (0,0)\n",
            fmt_mm(-minx),
            fmt_mm(-miny)
        ));
    }
    // Interior polylines: axis-aligned 4-vertex rects transcribe to
    // `cutout` statements; anything else is flagged for the designer.
    let mut cutout_lines: Vec<String> = Vec::new();
    let mut odd_interior = 0usize;
    for (k, pl) in dxf.polylines.iter().enumerate() {
        if k == outline_idx {
            continue;
        }
        // Same frame transform as the outline: flip, then translate.
        let pl: Vec<(f64, f64)> = pl
            .iter()
            .map(|&(x, y)| (x, if flip_y { -y } else { y }))
            .collect();
        let rect4 = pl.len() == 4
            && (0..4).all(|i| {
                let (ax, ay) = pl[i];
                let (bx, by) = pl[(i + 1) % 4];
                (ax - bx).abs() < 1e-9 || (ay - by).abs() < 1e-9
            });
        if rect4 {
            let xs: Vec<f64> = pl.iter().map(|p| p.0 - minx).collect();
            let ys: Vec<f64> = pl.iter().map(|p| p.1 - miny).collect();
            let (x0, x1) = (
                xs.iter().cloned().fold(f64::INFINITY, f64::min),
                xs.iter().cloned().fold(0.0_f64, f64::max),
            );
            let (y0, y1) = (
                ys.iter().cloned().fold(f64::INFINITY, f64::min),
                ys.iter().cloned().fold(0.0_f64, f64::max),
            );
            cutout_lines.push(format!(
                "    cutout rect ({}, {}) ({}, {});\n",
                fmt_mm(x0),
                fmt_mm(y0),
                fmt_mm(x1),
                fmt_mm(y1)
            ));
        } else {
            odd_interior += 1;
        }
    }
    if odd_interior > 0 {
        out.push_str(&format!(
            "// NOTE: {odd_interior} non-rectangular interior polyline(s) in the DXF \
             cannot be transcribed (only rect cutouts are supported)\n"
        ));
    }
    out.push_str("layout <Board> {\n");

    // Axis-aligned 4-vertex polygon degrades to the simpler rect form.
    let is_rect = pts.len() == 4 && {
        let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        (0..4).all(|i| {
            let j = (i + 1) % 4;
            (xs[i] - xs[j]).abs() < 1e-9 || (ys[i] - ys[j]).abs() < 1e-9
        })
    };
    if is_rect {
        let w = pts.iter().map(|p| p.0).fold(0.0_f64, f64::max);
        let h = pts.iter().map(|p| p.1).fold(0.0_f64, f64::max);
        out.push_str(&format!("    outline rect {} {};\n", fmt_mm(w), fmt_mm(h)));
    } else {
        out.push_str("    outline polygon");
        for &(x, y) in &pts {
            out.push_str(&format!(" ({}, {})", fmt_mm(x), fmt_mm(y)));
        }
        out.push_str(";\n");
    }
    for &(x, y, d) in &holes {
        out.push_str(&format!(
            "    mounting_hole ({}, {}) drill {} keepout 2.0; // keepout: DXF carries none — set per chassis\n",
            fmt_mm(x),
            fmt_mm(y),
            fmt_mm(d)
        ));
    }
    for line in &cutout_lines {
        out.push_str(line);
    }
    out.push_str(&format!("    mech_check \"{dxf_name}\";\n"));
    out.push_str("}\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const L_SHAPE: [(f64, f64); 6] = [
        (0.0, 0.0),
        (40.0, 0.0),
        (40.0, 18.0),
        (28.0, 18.0),
        (28.0, 30.0),
        (0.0, 30.0),
    ];

    fn dxf_with(pl: Vec<(f64, f64)>, circles: Vec<(f64, f64, f64)>) -> Dxf {
        Dxf { polylines: vec![pl], circles }
    }

    #[test]
    fn direct_frame_matches() {
        let dxf = dxf_with(L_SHAPE.to_vec(), vec![(36.0, 4.0, 1.6)]);
        check_parity(&L_SHAPE, &[(36.0, 4.0, 3.2)], &[], &dxf, 30.0).unwrap();
    }

    #[test]
    fn y_mirrored_frame_matches() {
        // MCAD Y-up: y -> 30 - y for every feature.
        let pl: Vec<_> = L_SHAPE.iter().map(|&(x, y)| (x, 30.0 - y)).collect();
        let dxf = dxf_with(pl, vec![(36.0, 26.0, 1.6)]);
        check_parity(&L_SHAPE, &[(36.0, 4.0, 3.2)], &[], &dxf, 30.0).unwrap();
    }

    #[test]
    fn outline_drift_fails() {
        let mut pl = L_SHAPE.to_vec();
        pl[2] = (40.0, 18.5); // 0.5mm notch drift
        let dxf = dxf_with(pl, vec![(36.0, 4.0, 1.6)]);
        assert!(check_parity(&L_SHAPE, &[(36.0, 4.0, 3.2)], &[], &dxf, 30.0).is_err());
    }

    #[test]
    fn hole_drill_drift_fails() {
        // Hole present but wrong drill (M3 vs declared 3.2).
        let dxf = dxf_with(L_SHAPE.to_vec(), vec![(36.0, 4.0, 1.5)]);
        assert!(check_parity(&L_SHAPE, &[(36.0, 4.0, 3.2)], &[], &dxf, 30.0).is_err());
    }

    #[test]
    fn cutout_parity_roundtrip() {
        let slot = vec![(10.0, 10.0), (20.0, 10.0), (20.0, 12.0), (10.0, 12.0)];
        let dxf = Dxf {
            polylines: vec![L_SHAPE.to_vec(), slot],
            circles: vec![(36.0, 4.0, 1.6)],
        };
        check_parity(
            &L_SHAPE,
            &[(36.0, 4.0, 3.2)],
            &[(10.0, 10.0, 20.0, 12.0)],
            &dxf,
            30.0,
        )
        .unwrap();
    }

    #[test]
    fn undeclared_aperture_fails() {
        let slot = vec![(10.0, 10.0), (20.0, 10.0), (20.0, 12.0), (10.0, 12.0)];
        let dxf = Dxf {
            polylines: vec![L_SHAPE.to_vec(), slot],
            circles: vec![(36.0, 4.0, 1.6)],
        };
        let e = check_parity(&L_SHAPE, &[(36.0, 4.0, 3.2)], &[], &dxf, 30.0)
            .unwrap_err()
            .to_string();
        assert!(e.contains("not declared"), "{e}");
    }

    #[test]
    fn missing_cutout_in_dxf_fails() {
        let dxf = dxf_with(L_SHAPE.to_vec(), vec![(36.0, 4.0, 1.6)]);
        let e = check_parity(
            &L_SHAPE,
            &[(36.0, 4.0, 3.2)],
            &[(10.0, 10.0, 20.0, 12.0)],
            &dxf,
            30.0,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("declared cutout"), "{e}");
    }

    #[test]
    fn import_emits_cutout() {
        let slot = vec![(10.0, 10.0), (20.0, 10.0), (20.0, 12.0), (10.0, 12.0)];
        let dxf = Dxf {
            polylines: vec![L_SHAPE.to_vec(), slot],
            circles: vec![],
        };
        let out = render_import(&dxf, "b.dxf", false).unwrap();
        assert!(out.contains("cutout rect (10, 10) (20, 12);"), "{out}");
    }

    #[test]
    fn import_renders_l_shape() {
        let dxf = dxf_with(L_SHAPE.to_vec(), vec![(36.0, 4.0, 1.6)]);
        let out = render_import(&dxf, "board.dxf", false).unwrap();
        assert!(out.contains(
            "outline polygon (0, 0) (40, 0) (40, 18) (28, 18) (28, 30) (0, 30);"
        ));
        assert!(out.contains("mounting_hole (36, 4) drill 3.2 keepout 2.0;"));
        assert!(out.contains("mech_check \"board.dxf\";"));
    }

    #[test]
    fn import_flip_y_matches_direct() {
        // A Y-up MCAD export imported with --flip-y must transcribe to
        // the same board-frame statements as a Y-down DXF without it.
        let up: Vec<_> = L_SHAPE.iter().map(|&(x, y)| (x, 30.0 - y)).collect();
        let a = render_import(
            &dxf_with(up, vec![(36.0, 26.0, 1.6)]),
            "b.dxf",
            true,
        )
        .unwrap();
        let b = render_import(
            &dxf_with(L_SHAPE.to_vec(), vec![(36.0, 4.0, 1.6)]),
            "b.dxf",
            false,
        )
        .unwrap();
        // The flip introduces a translation provenance comment; the
        // emitted STATEMENTS must be identical.
        let stmts = |s: &str| -> Vec<String> {
            s.lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .map(|l| l.to_string())
                .collect()
        };
        assert_eq!(stmts(&a), stmts(&b));
    }

    #[test]
    fn import_detects_rect() {
        let rect = vec![(5.0, 5.0), (51.0, 5.0), (51.0, 37.0), (5.0, 37.0)];
        let out = render_import(&dxf_with(rect, vec![]), "r.dxf", false).unwrap();
        assert!(out.contains("outline rect 46 32;"), "{out}");
        assert!(out.contains("translated by (-5, -5)"));
    }

    #[test]
    fn parses_ascii_dxf() {
        let dir = std::env::temp_dir().join("bhdl_mech_test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.dxf");
        std::fs::write(
            &p,
            "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n90\n3\n70\n1\n\
             10\n0.0\n20\n0.0\n10\n10.0\n20\n0.0\n10\n0.0\n20\n10.0\n\
             0\nCIRCLE\n8\n0\n10\n5.0\n20\n5.0\n40\n1.6\n0\nENDSEC\n0\nEOF\n",
        )
        .unwrap();
        let dxf = parse_dxf(&p).unwrap();
        assert_eq!(dxf.polylines, vec![vec![(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]]);
        assert_eq!(dxf.circles, vec![(5.0, 5.0, 1.6)]);
    }
}
