//! Fab-house preflight: measure the SHIPPED copper against a named
//! manufacturing capability profile.
//!
//! DRC (KiCad's or ours) proves the board is consistent with its own
//! design rules; it says nothing about whether a fab can build it. A
//! 0.15 mm trace passes DRC on a board whose rules say 0.15 and comes
//! back from a standard prototype line as an open. Preflight measures
//! the things a capability sheet lists — trace width, spacing, drill,
//! annular ring, hole-to-hole, copper-to-edge — against a profile the
//! user names, and reports every violation with the measured value.
//!
//! Generic by construction: profiles are data, the checks are the same
//! for every board, and nothing here knows about any specific design.

use crate::types::{Board, Route};

/// A fab's published minimums. All in mm. Every field is a floor
/// (larger is always acceptable).
#[derive(Debug, Clone, PartialEq)]
pub struct FabProfile {
    pub name: &'static str,
    /// Minimum finished trace width.
    pub min_trace_mm: f64,
    /// Minimum copper-to-copper spacing.
    pub min_space_mm: f64,
    /// Minimum finished (round) drill.
    pub min_drill_mm: f64,
    /// Minimum via drill (some fabs allow smaller via drills than pad drills).
    pub min_via_drill_mm: f64,
    /// Minimum annular ring: (pad - drill) / 2.
    pub min_annular_mm: f64,
    /// Minimum hole edge to hole edge.
    pub min_hole_to_hole_mm: f64,
    /// Minimum copper to board edge.
    pub min_copper_to_edge_mm: f64,
}

impl FabProfile {
    /// Conservative standard 2-layer prototype service (the common
    /// "6/6 mil, 0.3 mm drill" tier). The default when nothing is
    /// named — a board that passes this ships almost anywhere.
    pub const STANDARD: FabProfile = FabProfile {
        name: "standard",
        min_trace_mm: 0.152,
        min_space_mm: 0.152,
        min_drill_mm: 0.30,
        min_via_drill_mm: 0.30,
        min_annular_mm: 0.13,
        min_hole_to_hole_mm: 0.25,
        min_copper_to_edge_mm: 0.30,
    };

    /// Fine-line tier ("4/4 mil, 0.2 mm drill") — the usual paid upgrade.
    pub const FINE: FabProfile = FabProfile {
        name: "fine",
        min_trace_mm: 0.10,
        min_space_mm: 0.10,
        min_drill_mm: 0.20,
        min_via_drill_mm: 0.20,
        min_annular_mm: 0.10,
        min_hole_to_hole_mm: 0.20,
        min_copper_to_edge_mm: 0.25,
    };

    /// Coarse hobby/etch tier ("10/10 mil, 0.5 mm drill").
    pub const COARSE: FabProfile = FabProfile {
        name: "coarse",
        min_trace_mm: 0.254,
        min_space_mm: 0.254,
        min_drill_mm: 0.50,
        min_via_drill_mm: 0.50,
        min_annular_mm: 0.20,
        min_hole_to_hole_mm: 0.40,
        min_copper_to_edge_mm: 0.50,
    };

    pub fn by_name(name: &str) -> Option<FabProfile> {
        match name.to_ascii_lowercase().as_str() {
            "standard" | "std" | "default" => Some(Self::STANDARD),
            "fine" => Some(Self::FINE),
            "coarse" | "hobby" => Some(Self::COARSE),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PreflightKind {
    TraceWidth,
    DesignSpacing,
    Drill,
    ViaDrill,
    AnnularRing,
    HoleToHole,
    CopperToEdge,
}

#[derive(Debug, Clone)]
pub struct PreflightFinding {
    pub kind: PreflightKind,
    /// Human-readable subject: "net X segment", "via at (x,y)", "R3 pad 2".
    pub subject: String,
    pub measured_mm: f64,
    pub required_mm: f64,
    pub at: (f64, f64),
}

#[derive(Debug, Clone)]
pub struct PreflightReport {
    pub profile: FabProfile,
    pub findings: Vec<PreflightFinding>,
    /// Counts of what was measured, so an empty findings list can be
    /// told apart from a check that measured nothing.
    pub segments_checked: usize,
    pub vias_checked: usize,
    pub holes_checked: usize,
}

impl PreflightReport {
    pub fn pass(&self) -> bool {
        self.findings.is_empty()
    }
}

/// One drilled hole on the shipped board (pad or via), for the
/// hole-to-hole and drill checks.
struct Hole {
    x: f64,
    y: f64,
    /// Round drill diameter, or the slot's minor axis.
    drill_mm: f64,
    /// Slot major axis (== drill_mm when round).
    major_mm: f64,
    /// Slot as a capsule: unit direction of the long axis (board
    /// frame) and half the straight length (major - minor)/2. Zero
    /// for round holes, so the capsule degenerates to the circle.
    axis: (f64, f64),
    half_len: f64,
    /// Outer copper diameter along the minor axis (annular ring base).
    pad_minor_mm: f64,
    subject: String,
    is_via: bool,
}

pub fn preflight(board: &Board, routes: &[Route], profile: &FabProfile) -> PreflightReport {
    let mut findings = Vec::new();
    let eps = 1e-6;
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    let has_outline = bw > 0.0 && bh > 0.0;

    // ── Traces ────────────────────────────────────────────────────
    let mut segments_checked = 0usize;
    for r in routes {
        let net_name = board
            .nets
            .iter()
            .find(|n| n.id == r.net_id)
            .map(|n| n.name.as_str())
            .unwrap_or("?");
        for sg in &r.segments {
            segments_checked += 1;
            if sg.width_mm + eps < profile.min_trace_mm {
                findings.push(PreflightFinding {
                    kind: PreflightKind::TraceWidth,
                    subject: format!("net '{net_name}' track"),
                    measured_mm: sg.width_mm,
                    required_mm: profile.min_trace_mm,
                    at: sg.start,
                });
            }
            if has_outline {
                let half = sg.width_mm / 2.0;
                for &(x, y) in &[sg.start, sg.end] {
                    let d = edge_distance(&board.config.outline, x, y) - half;
                    if d + eps < profile.min_copper_to_edge_mm {
                        findings.push(PreflightFinding {
                            kind: PreflightKind::CopperToEdge,
                            subject: format!("net '{net_name}' track end"),
                            measured_mm: d.max(0.0),
                            required_mm: profile.min_copper_to_edge_mm,
                            at: (x, y),
                        });
                    }
                }
            }
        }
    }

    // ── Design spacing: the router honours board.config.min_spacing_mm
    // between copper; if that floor is below the fab's, every legal
    // gap on the board may be unbuildable. One finding, not thousands.
    if board.config.min_spacing_mm + eps < profile.min_space_mm {
        findings.push(PreflightFinding {
            kind: PreflightKind::DesignSpacing,
            subject: "board design clearance".to_string(),
            measured_mm: board.config.min_spacing_mm,
            required_mm: profile.min_space_mm,
            at: (0.0, 0.0),
        });
    }

    // ── Holes: pads + vias ────────────────────────────────────────
    let mut holes: Vec<Hole> = Vec::new();
    for comp in &board.components {
        let (cos_t, sin_t) = (comp.theta.cos(), comp.theta.sin());
        for pin in &comp.pins {
            if pin.unplaced {
                continue;
            }
            let Some(pad) = pin.pad.as_ref() else { continue };
            let Some(drill) = pad.drill_mm else { continue };
            let x = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let y = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let (minor, major) = match pad.drill_slot_mm {
                Some((w, h)) => (w.min(h), w.max(h)),
                None => (drill, drill),
            };
            // Annular ring base: copper extent along the hole's minor
            // axis. For a slot the ring is measured across the slot's
            // narrow direction; pad width/height are footprint-local
            // and the slot is aligned with them.
            let pad_minor = match pad.drill_slot_mm {
                Some((w, h)) => {
                    if w <= h { pad.width_mm } else { pad.height_mm }
                }
                None => pad.width_mm.min(pad.height_mm),
            };
            // Slot long axis in the board frame: footprint-local x if
            // the slot is wider than tall, else local y, rotated by
            // the component's theta.
            let (axis, half_len) = match pad.drill_slot_mm {
                Some((w, h)) => {
                    let (lx, ly) = if w >= h { (1.0, 0.0) } else { (0.0, 1.0) };
                    ((lx * cos_t - ly * sin_t, lx * sin_t + ly * cos_t), (major - minor) / 2.0)
                }
                None => ((1.0, 0.0), 0.0),
            };
            holes.push(Hole {
                x,
                y,
                drill_mm: minor,
                major_mm: major,
                axis,
                half_len,
                pad_minor_mm: pad_minor,
                subject: format!("{} pad {}", comp.name, pin.name),
                is_via: false,
            });
        }
    }
    let via = &board.layer_stack.via;
    let mut vias_checked = 0usize;
    for r in routes {
        for v in &r.vias {
            vias_checked += 1;
            holes.push(Hole {
                x: v.x,
                y: v.y,
                drill_mm: via.drill_mm,
                major_mm: via.drill_mm,
                axis: (1.0, 0.0),
                half_len: 0.0,
                pad_minor_mm: via.pad_mm,
                subject: format!("via at ({:.2},{:.2})", v.x, v.y),
                is_via: true,
            });
        }
    }
    let holes_checked = holes.len();

    for h in &holes {
        let (kind, floor) = if h.is_via {
            (PreflightKind::ViaDrill, profile.min_via_drill_mm)
        } else {
            (PreflightKind::Drill, profile.min_drill_mm)
        };
        if h.drill_mm + eps < floor {
            findings.push(PreflightFinding {
                kind,
                subject: h.subject.clone(),
                measured_mm: h.drill_mm,
                required_mm: floor,
                at: (h.x, h.y),
            });
        }
        let ring = (h.pad_minor_mm - h.drill_mm) / 2.0;
        if ring + eps < profile.min_annular_mm {
            findings.push(PreflightFinding {
                kind: PreflightKind::AnnularRing,
                subject: h.subject.clone(),
                measured_mm: ring.max(0.0),
                required_mm: profile.min_annular_mm,
                at: (h.x, h.y),
            });
        }
        if has_outline {
            let d = edge_distance(&board.config.outline, h.x, h.y) - h.pad_minor_mm.max(h.major_mm) / 2.0;
            if d + eps < profile.min_copper_to_edge_mm {
                findings.push(PreflightFinding {
                    kind: PreflightKind::CopperToEdge,
                    subject: h.subject.clone(),
                    measured_mm: d.max(0.0),
                    required_mm: profile.min_copper_to_edge_mm,
                    at: (h.x, h.y),
                });
            }
        }
    }

    // Hole-to-hole: edge-to-edge between every pair, each hole modelled
    // as a capsule (slot) or circle (round), so a via beside a slot's
    // narrow face is measured to that face — treating a 3.6 mm slot as
    // a 3.6 mm circle flagged a via 1.07 mm from a slot as 0.000 mm.
    // O(n²) over holes is fine at board scale (hundreds).
    for i in 0..holes.len() {
        for j in (i + 1)..holes.len() {
            let (a, b) = (&holes[i], &holes[j]);
            let c = capsule_distance(a, b);
            let d = c - a.drill_mm / 2.0 - b.drill_mm / 2.0;
            if d + eps < profile.min_hole_to_hole_mm {
                findings.push(PreflightFinding {
                    kind: PreflightKind::HoleToHole,
                    subject: format!("{} vs {}", a.subject, b.subject),
                    measured_mm: d.max(0.0),
                    required_mm: profile.min_hole_to_hole_mm,
                    at: ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
                });
            }
        }
    }

    PreflightReport {
        profile: profile.clone(),
        findings,
        segments_checked,
        vias_checked,
        holes_checked,
    }
}

/// Distance between the centre-lines of two capsules (segment-to-
/// segment); each hole's edge is then drill/2 beyond its centre-line.
fn capsule_distance(a: &Hole, b: &Hole) -> f64 {
    let (a0, a1) = (
        (a.x - a.axis.0 * a.half_len, a.y - a.axis.1 * a.half_len),
        (a.x + a.axis.0 * a.half_len, a.y + a.axis.1 * a.half_len),
    );
    let (b0, b1) = (
        (b.x - b.axis.0 * b.half_len, b.y - b.axis.1 * b.half_len),
        (b.x + b.axis.0 * b.half_len, b.y + b.axis.1 * b.half_len),
    );
    // Segment-segment distance: min over endpoint-to-segment in both
    // directions (exact for non-crossing segments; crossing = 0).
    if segments_cross(a0, a1, b0, b1) {
        return 0.0;
    }
    point_seg(a0, b0, b1)
        .min(point_seg(a1, b0, b1))
        .min(point_seg(b0, a0, a1))
        .min(point_seg(b1, a0, a1))
}

fn point_seg(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 1e-12 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

fn segments_cross(a0: (f64, f64), a1: (f64, f64), b0: (f64, f64), b1: (f64, f64)) -> bool {
    let cross = |o: (f64, f64), p: (f64, f64), q: (f64, f64)| {
        (p.0 - o.0) * (q.1 - o.1) - (p.1 - o.1) * (q.0 - o.0)
    };
    let d1 = cross(b0, b1, a0);
    let d2 = cross(b0, b1, a1);
    let d3 = cross(a0, a1, b0);
    let d4 = cross(a0, a1, b1);
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// Distance from a point to the nearest board edge (0 outside).
fn edge_distance(outline: &crate::types::BoardOutline, x: f64, y: f64) -> f64 {
    use crate::types::BoardOutline;
    match outline {
        BoardOutline::Rectangle { width_mm, height_mm } => {
            let d = x.min(width_mm - x).min(y).min(height_mm - y);
            d.max(0.0)
        }
        BoardOutline::Polygon(pts) => {
            if !outline.contains(x, y) {
                return 0.0;
            }
            let n = pts.len();
            let mut best = f64::INFINITY;
            for i in 0..n {
                let (ax, ay) = pts[i];
                let (bx, by) = pts[(i + 1) % n];
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = dx * dx + dy * dy;
                let t = if len2 <= 1e-12 {
                    0.0
                } else {
                    (((x - ax) * dx + (y - ay) * dy) / len2).clamp(0.0, 1.0)
                };
                let (cx, cy) = (ax + t * dx, ay + t * dy);
                best = best.min((x - cx).hypot(y - cy));
            }
            best
        }
        BoardOutline::AutoSize => f64::INFINITY,
    }
}

/// Print the report; returns pass/fail.
pub fn print_report(report: &PreflightReport) -> bool {
    println!(
        "\n  Fab preflight [{}]: {} track(s), {} via(s), {} hole(s) measured",
        report.profile.name, report.segments_checked, report.vias_checked, report.holes_checked
    );
    if report.findings.is_empty() {
        println!("    no violations");
    } else {
        // Group by kind for a readable summary, then list the first few
        // of each with the measured value.
        let kinds = [
            (PreflightKind::TraceWidth, "trace width"),
            (PreflightKind::DesignSpacing, "design spacing"),
            (PreflightKind::Drill, "drill"),
            (PreflightKind::ViaDrill, "via drill"),
            (PreflightKind::AnnularRing, "annular ring"),
            (PreflightKind::HoleToHole, "hole-to-hole"),
            (PreflightKind::CopperToEdge, "copper-to-edge"),
        ];
        for (k, label) in kinds.iter() {
            let of_kind: Vec<&PreflightFinding> =
                report.findings.iter().filter(|f| &f.kind == k).collect();
            if of_kind.is_empty() {
                continue;
            }
            println!("    ✗ {} ({}):", label, of_kind.len());
            for f in of_kind.iter().take(5) {
                println!(
                    "      {} — {:.3} mm < {:.3} mm at ({:.2}, {:.2})",
                    f.subject, f.measured_mm, f.required_mm, f.at.0, f.at.1
                );
            }
            if of_kind.len() > 5 {
                println!("      ... and {} more", of_kind.len() - 5);
            }
        }
    }
    let pass = report.pass();
    println!("  Preflight: {}", if pass { "PASS" } else { "FAIL" });
    pass
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_resolve_by_name() {
        assert_eq!(FabProfile::by_name("standard").unwrap().name, "standard");
        assert_eq!(FabProfile::by_name("FINE").unwrap().name, "fine");
        assert_eq!(FabProfile::by_name("hobby").unwrap().name, "coarse");
        assert!(FabProfile::by_name("nope").is_none());
    }

    fn hole(x: f64, y: f64, drill: f64, slot: Option<((f64, f64), f64)>) -> Hole {
        let (axis, half_len) = slot.unwrap_or(((1.0, 0.0), 0.0));
        Hole { x, y, drill_mm: drill, major_mm: drill + 2.0 * half_len, axis, half_len,
               pad_minor_mm: drill + 0.6, subject: String::new(), is_via: false }
    }

    #[test]
    fn slot_is_a_capsule_not_a_circle() {
        // 3.6 x 1.0 slot along x at origin; via 1.72 mm above its centre.
        let slot = hole(0.0, 0.0, 1.0, Some(((1.0, 0.0), 1.3)));
        let via = hole(0.32, 1.72, 0.3, None);
        let d = capsule_distance(&slot, &via) - 0.5 - 0.15;
        assert!((d - 1.07).abs() < 1e-9, "edge gap {d}");
        // Same via beside the slot's END: 1.3 + 0.5 = 1.8 to the tip.
        let via2 = hole(2.5, 0.0, 0.3, None);
        let d2 = capsule_distance(&slot, &via2) - 0.5 - 0.15;
        assert!((d2 - 0.55).abs() < 1e-9, "end gap {d2}");
        // A via inside the slot is 0.
        let via3 = hole(0.5, 0.0, 0.3, None);
        assert!(capsule_distance(&slot, &via3) < 1e-9);
    }

    #[test]
    fn edge_distance_rect_and_polygon() {
        use crate::types::BoardOutline;
        let r = BoardOutline::Rectangle { width_mm: 10.0, height_mm: 5.0 };
        assert!((edge_distance(&r, 1.0, 2.5) - 1.0).abs() < 1e-9);
        assert!((edge_distance(&r, 9.0, 4.0) - 1.0).abs() < 1e-9);
        assert_eq!(edge_distance(&r, -1.0, 2.0), 0.0);
        let p = BoardOutline::Polygon(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)]);
        assert!((edge_distance(&p, 1.0, 2.5) - 1.0).abs() < 1e-9);
        assert_eq!(edge_distance(&p, 11.0, 2.0), 0.0);
    }
}
