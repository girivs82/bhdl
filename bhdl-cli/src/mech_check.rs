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
    dxf: &Dxf,
    board_h: f64,
) -> Result<()> {
    const TOL: f64 = 0.05;
    let mirror = |pts: &[(f64, f64)]| -> Vec<(f64, f64)> {
        pts.iter().map(|&(x, y)| (x, board_h - y)).collect()
    };

    // Outline: match against any DXF polyline, direct or Y-mirrored.
    let mut outline_ok = None;
    for pl in &dxf.polylines {
        if points_match(outline_pts, pl, TOL) {
            outline_ok = Some("direct");
            break;
        }
        if points_match(&mirror(outline_pts), pl, TOL) {
            outline_ok = Some("y-mirrored");
            break;
        }
    }
    let frame = match outline_ok {
        Some(f) => f,
        None => bail!(
            "mech_check FAILED: board outline ({} vertices) matches no DXF \
             polyline (tried direct and y-mirrored frames, tol {TOL}mm)",
            outline_pts.len()
        ),
    };

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
        "  ✓ Mech parity: outline {} vertices ✓, holes {}/{} ✓ ({} frame)",
        outline_pts.len(),
        holes.len(),
        holes.len(),
        frame
    );
    Ok(())
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
        check_parity(&L_SHAPE, &[(36.0, 4.0, 3.2)], &dxf, 30.0).unwrap();
    }

    #[test]
    fn y_mirrored_frame_matches() {
        // MCAD Y-up: y -> 30 - y for every feature.
        let pl: Vec<_> = L_SHAPE.iter().map(|&(x, y)| (x, 30.0 - y)).collect();
        let dxf = dxf_with(pl, vec![(36.0, 26.0, 1.6)]);
        check_parity(&L_SHAPE, &[(36.0, 4.0, 3.2)], &dxf, 30.0).unwrap();
    }

    #[test]
    fn outline_drift_fails() {
        let mut pl = L_SHAPE.to_vec();
        pl[2] = (40.0, 18.5); // 0.5mm notch drift
        let dxf = dxf_with(pl, vec![(36.0, 4.0, 1.6)]);
        assert!(check_parity(&L_SHAPE, &[(36.0, 4.0, 3.2)], &dxf, 30.0).is_err());
    }

    #[test]
    fn hole_drill_drift_fails() {
        // Hole present but wrong drill (M3 vs declared 3.2).
        let dxf = dxf_with(L_SHAPE.to_vec(), vec![(36.0, 4.0, 1.5)]);
        assert!(check_parity(&L_SHAPE, &[(36.0, 4.0, 3.2)], &dxf, 30.0).is_err());
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
