//! IPC-7351B land pattern generator.
//!
//! Generates PCB footprints (pad positions and sizes) from component body
//! dimensions using the IPC-7351B standard formulas. Component dimensions
//! come from JEDEC standards (EIA, JEDEC MO/MS outlines).
//!
//! The core formulas compute three tolerance zone dimensions:
//!
//! ```text
//! Zmax = Lmin + 2·Jt + √(CL² + F² + P²)   // outer pad-to-pad span
//! Gmin = Smax - 2·Jh - √(CS² + F² + P²)   // inner gap between pads
//! Xmax = Wmin + 2·Js + √(CW² + F² + P²)   // pad width
//! ```
//!
//! From which pad geometry is derived:
//!
//! ```text
//! pad_length   = (Zmax - Gmin) / 2
//! pad_width    = Xmax
//! pad_center_x = ±(Zmax + Gmin) / 4
//! ```

use bhdl_components::ComponentFootprint;
use bhdl_components::types::component::{FootprintPad, PadShape, PadType};

// ── Constants ────────────────────────────────────────────────────────

/// PCB fabrication tolerance (mm).
const F: f64 = 0.05;
/// Component placement tolerance (mm).
const P: f64 = 0.025;
/// Rounding grid for pad dimensions (mm).
const ROUND_GRID: f64 = 0.05;

// ── Public types ─────────────────────────────────────────────────────

/// IPC-7351B density level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DensityLevel {
    /// Level A — most land protrusion (hand soldering, rework friendly).
    Most,
    /// Level B — nominal (standard reflow).
    Nominal,
    /// Level C — least land protrusion (high density).
    Least,
}

/// Solder fillet goals for toe, heel, and side.
#[derive(Debug, Clone, Copy)]
struct JValues {
    jt: f64, // toe
    jh: f64, // heel
    js: f64, // side
}

/// Component package family with physical dimensions from JEDEC standards.
#[derive(Debug, Clone)]
pub enum PackageFamily {
    /// 2-terminal chip (resistors, capacitors, inductors): 0201–2512.
    Chip {
        l: f64,   // body length (lead tip to lead tip), mm
        w: f64,   // body width, mm
        t: f64,   // terminal/end cap length, mm
        tol: f64, // dimensional tolerance ±, mm
    },
    /// Gull-wing leaded (SOIC, SOP, SOT-23, SOT-223).
    GullWing {
        body: (f64, f64),       // (length, width) of plastic body, mm
        span: f64,              // lead tip to lead tip across body, mm
        pitch: f64,             // pin spacing, mm
        lead_width: f64,        // individual lead width, mm
        pins: usize,            // total pin count
        pins_per_side: Vec<usize>, // pins on each side [left, right] or [left, right, tab]
    },
    /// Quad flat package (QFP, TQFP, LQFP).
    QuadFlat {
        body: f64,       // square body side, mm
        span: f64,       // lead tip to lead tip, mm
        pitch: f64,      // pin spacing, mm
        lead_width: f64, // lead width, mm
        pins: usize,     // total pins (must be divisible by 4)
    },
    /// Quad flat no-lead (QFN, DFN).
    QFN {
        body: f64,        // square body side, mm
        pitch: f64,       // pin spacing, mm
        lead_width: f64,  // lead/pad width, mm
        lead_len: f64,    // exposed lead length from edge, mm
        pins: usize,      // perimeter pin count (excluding epad)
        epad: Option<(f64, f64)>, // exposed/thermal pad (w, h), mm
    },
    /// DPAK / D2PAK / TO-252 / TO-263.
    DPAK {
        body: (f64, f64),  // (length, width), mm
        tab: (f64, f64),   // tab/drain pad (width, length), mm
        pitch: f64,        // lead spacing, mm
        lead_width: f64,   // lead width, mm
        lead_span: f64,    // lead tip to tab back, mm
        pins: usize,       // lead pins (excluding tab)
    },
}

// ── J-value tables ───────────────────────────────────────────────────

fn chip_j(level: DensityLevel) -> JValues {
    match level {
        DensityLevel::Most    => JValues { jt: 0.55, jh: 0.00, js: 0.05 },
        DensityLevel::Nominal => JValues { jt: 0.35, jh: 0.00, js: 0.00 },
        DensityLevel::Least   => JValues { jt: 0.15, jh: 0.00, js: -0.05 },
    }
}

fn gullwing_large_pitch_j(level: DensityLevel) -> JValues {
    // Pitch > 0.625mm: SOIC, SOT-23, SOT-223
    match level {
        DensityLevel::Most    => JValues { jt: 0.55, jh: 0.45, js: 0.05 },
        DensityLevel::Nominal => JValues { jt: 0.35, jh: 0.35, js: 0.03 },
        DensityLevel::Least   => JValues { jt: 0.15, jh: 0.25, js: 0.01 },
    }
}

fn gullwing_small_pitch_j(level: DensityLevel) -> JValues {
    // Pitch <= 0.625mm: TQFP, LQFP fine-pitch
    match level {
        DensityLevel::Most    => JValues { jt: 0.55, jh: 0.45, js: 0.01 },
        DensityLevel::Nominal => JValues { jt: 0.35, jh: 0.35, js: -0.02 },
        DensityLevel::Least   => JValues { jt: 0.15, jh: 0.25, js: -0.04 },
    }
}

fn qfn_j(level: DensityLevel) -> JValues {
    match level {
        DensityLevel::Most    => JValues { jt: 0.40, jh: 0.00, js: -0.04 },
        DensityLevel::Nominal => JValues { jt: 0.30, jh: 0.00, js: -0.04 },
        DensityLevel::Least   => JValues { jt: 0.20, jh: 0.00, js: -0.04 },
    }
}

// ── Core IPC formula ─────────────────────────────────────────────────

/// Compute the three IPC-7351B tolerance zone dimensions.
///
/// Returns `(Zmax, Gmin, Xmax)` in mm.
fn compute_zgx(
    l_min: f64,  // minimum component span (lead tip to lead tip)
    s_max: f64,  // maximum gap between leads (inner edges)
    w_min: f64,  // minimum lead width
    cl: f64,     // component length tolerance
    cs: f64,     // component gap tolerance
    cw: f64,     // lead width tolerance
    j: &JValues,
) -> (f64, f64, f64) {
    let rss_l = (cl * cl + F * F + P * P).sqrt();
    let rss_s = (cs * cs + F * F + P * P).sqrt();
    let rss_w = (cw * cw + F * F + P * P).sqrt();

    let z = l_min + 2.0 * j.jt + rss_l;
    let g = s_max - 2.0 * j.jh - rss_s;
    let x = w_min + 2.0 * j.js + rss_w;

    (round_to(z, ROUND_GRID), round_to(g, ROUND_GRID), round_to(x, ROUND_GRID))
}

/// Round to nearest grid increment.
fn round_to(val: f64, grid: f64) -> f64 {
    (val / grid).round() * grid
}

// ── Pad helpers ──────────────────────────────────────────────────────

fn make_pad(number: &str, x: f64, y: f64, w: f64, h: f64) -> FootprintPad {
    FootprintPad {
        pad_number: number.to_string(),
        x_position: round_to(x, 0.001),
        y_position: round_to(y, 0.001),
        width: round_to(w, ROUND_GRID),
        height: round_to(h, ROUND_GRID),
        shape: PadShape::Rectangle,
        drill_diameter: None,
        pad_type: PadType::SMD,
    }
}

// ── Family-specific generators ───────────────────────────────────────

fn generate_chip(l: f64, w: f64, t: f64, tol: f64, level: DensityLevel) -> ComponentFootprint {
    let j = chip_j(level);
    let l_min = l - tol;
    let s_max = l - 2.0 * t + tol; // gap = body - 2*terminal + tolerance
    let w_min = w - tol;
    let cl = 2.0 * tol;
    let cs = 2.0 * tol;
    let cw = 2.0 * tol;

    let (z, g, x) = compute_zgx(l_min, s_max, w_min, cl, cs, cw, &j);

    let pad_len = (z - g) / 2.0;
    let pad_wid = x;
    let center = (z + g) / 4.0;

    let pads = vec![
        make_pad("1", -center, 0.0, pad_len, pad_wid),
        make_pad("2", center, 0.0, pad_len, pad_wid),
    ];

    ComponentFootprint {
        footprint_name: format!("CHIP_{}x{}", (l * 100.0) as u32, (w * 100.0) as u32),
        svg_data: String::new(),
        pad_count: 2,
        body_width: l,
        body_height: w,
        pitch: None,
        pads,
    }
}

fn generate_gullwing(
    body: (f64, f64),
    span: f64,
    pitch: f64,
    lead_width: f64,
    pins: usize,
    pins_per_side: &[usize],
    level: DensityLevel,
) -> ComponentFootprint {
    let j = if pitch > 0.625 {
        gullwing_large_pitch_j(level)
    } else {
        gullwing_small_pitch_j(level)
    };

    // Lead dimensions for gull-wing: L = span, S = body_width (gap across body)
    let tol = 0.10; // typical for gull-wing
    let l_min = span - tol;
    let s_max = body.1 + tol; // body width is the gap between lead roots
    let w_min = lead_width - 0.05;
    let cl = 2.0 * tol;
    let cs = 2.0 * tol;
    let cw = 0.10;

    let (z, g, x) = compute_zgx(l_min, s_max, w_min, cl, cs, cw, &j);

    let pad_len = (z - g) / 2.0;
    let pad_wid = x;
    let center_x = (z + g) / 4.0;

    let mut pads = Vec::with_capacity(pins);

    // Left side
    let left_count = pins_per_side[0];
    let left_start_y = -(left_count as f64 - 1.0) * pitch / 2.0;
    for i in 0..left_count {
        let y = left_start_y + i as f64 * pitch;
        pads.push(make_pad(
            &(pads.len() + 1).to_string(),
            -center_x,
            y,
            pad_len,
            pad_wid,
        ));
    }

    // Right side (numbered bottom-to-top for standard IC pinout)
    let right_count = pins_per_side[1];
    let right_start_y = (right_count as f64 - 1.0) * pitch / 2.0;
    for i in 0..right_count {
        let y = right_start_y - i as f64 * pitch;
        pads.push(make_pad(
            &(pads.len() + 1).to_string(),
            center_x,
            y,
            pad_len,
            pad_wid,
        ));
    }

    // Tab pad (e.g., SOT-223 pin 4 / SOT-89 collector)
    if pins_per_side.len() > 2 && pins_per_side[2] > 0 {
        // Tab on the right side, large pad
        let tab_width = body.1 * 0.6; // ~60% of body width
        pads.push(make_pad(
            &(pads.len() + 1).to_string(),
            center_x,
            0.0,
            pad_len,
            tab_width,
        ));
    }

    ComponentFootprint {
        footprint_name: format!("GW_{}P{}", pins, (pitch * 100.0) as u32),
        svg_data: String::new(),
        pad_count: pads.len() as u32,
        body_width: body.0,
        body_height: body.1,
        pitch: Some(pitch),
        pads,
    }
}

fn generate_quad(
    body: f64,
    span: f64,
    pitch: f64,
    lead_width: f64,
    pins: usize,
    level: DensityLevel,
) -> ComponentFootprint {
    let j = if pitch > 0.625 {
        gullwing_large_pitch_j(level)
    } else {
        gullwing_small_pitch_j(level)
    };

    let tol = 0.10;
    let l_min = span - tol;
    let s_max = body + tol;
    let w_min = lead_width - 0.05;
    let cl = 2.0 * tol;
    let cs = 2.0 * tol;
    let cw = 0.10;

    let (z, g, x) = compute_zgx(l_min, s_max, w_min, cl, cs, cw, &j);

    let pad_len = (z - g) / 2.0;
    let pad_wid = x;
    let center = (z + g) / 4.0;

    let pps = pins / 4; // pins per side
    let mut pads = Vec::with_capacity(pins);

    // Left side (top to bottom)
    let start = -(pps as f64 - 1.0) * pitch / 2.0;
    for i in 0..pps {
        let y = start + i as f64 * pitch;
        pads.push(make_pad(
            &(pads.len() + 1).to_string(),
            -center,
            y,
            pad_len,
            pad_wid,
        ));
    }

    // Bottom side (left to right)
    for i in 0..pps {
        let x_pos = start + i as f64 * pitch;
        pads.push(make_pad(
            &(pads.len() + 1).to_string(),
            x_pos,
            center,
            pad_wid,
            pad_len,
        ));
    }

    // Right side (bottom to top)
    for i in 0..pps {
        let y = -start - i as f64 * pitch;
        pads.push(make_pad(
            &(pads.len() + 1).to_string(),
            center,
            y,
            pad_len,
            pad_wid,
        ));
    }

    // Top side (right to left)
    for i in 0..pps {
        let x_pos = -start - i as f64 * pitch;
        pads.push(make_pad(
            &(pads.len() + 1).to_string(),
            x_pos,
            -center,
            pad_wid,
            pad_len,
        ));
    }

    ComponentFootprint {
        footprint_name: format!("QFP{}P{}_{}", pins, (pitch * 100.0) as u32, (span * 100.0) as u32),
        svg_data: String::new(),
        pad_count: pads.len() as u32,
        body_width: body,
        body_height: body,
        pitch: Some(pitch),
        pads,
    }
}

fn generate_qfn(
    body: f64,
    pitch: f64,
    lead_width: f64,
    lead_len: f64,
    pins: usize,
    epad: Option<(f64, f64)>,
    level: DensityLevel,
) -> ComponentFootprint {
    let j = qfn_j(level);

    // QFN: leads are on the bottom, extending from body edge inward
    // L = body (leads flush with edge), S = body - 2*lead_len
    let tol = 0.05;
    let l_min = body - tol;
    let s_max = body - 2.0 * lead_len + tol;
    let w_min = lead_width - tol;
    let cl = 2.0 * tol;
    let cs = 2.0 * tol;
    let cw = 2.0 * tol;

    let (z, g, x) = compute_zgx(l_min, s_max, w_min, cl, cs, cw, &j);

    let pad_len = (z - g) / 2.0;
    let pad_wid = x;
    let center = (z + g) / 4.0;

    let pps = pins / 4;
    let mut pads = Vec::with_capacity(pins + if epad.is_some() { 1 } else { 0 });

    let start = -(pps as f64 - 1.0) * pitch / 2.0;

    // Left side (top to bottom)
    for i in 0..pps {
        let y = start + i as f64 * pitch;
        pads.push(make_pad(&(pads.len() + 1).to_string(), -center, y, pad_len, pad_wid));
    }

    // Bottom side (left to right)
    for i in 0..pps {
        let x_pos = start + i as f64 * pitch;
        pads.push(make_pad(&(pads.len() + 1).to_string(), x_pos, center, pad_wid, pad_len));
    }

    // Right side (bottom to top)
    for i in 0..pps {
        let y = -start - i as f64 * pitch;
        pads.push(make_pad(&(pads.len() + 1).to_string(), center, y, pad_len, pad_wid));
    }

    // Top side (right to left)
    for i in 0..pps {
        let x_pos = -start - i as f64 * pitch;
        pads.push(make_pad(&(pads.len() + 1).to_string(), x_pos, -center, pad_wid, pad_len));
    }

    // Exposed/thermal pad
    if let Some((ew, eh)) = epad {
        pads.push(make_pad(&(pads.len() + 1).to_string(), 0.0, 0.0, ew, eh));
    }

    ComponentFootprint {
        footprint_name: format!("QFN{}P{}_{}", pins, (pitch * 100.0) as u32, (body * 100.0) as u32),
        svg_data: String::new(),
        pad_count: pads.len() as u32,
        body_width: body,
        body_height: body,
        pitch: Some(pitch),
        pads,
    }
}

fn generate_dpak(
    body: (f64, f64),
    tab: (f64, f64),
    pitch: f64,
    lead_width: f64,
    lead_span: f64,
    pins: usize,
    level: DensityLevel,
) -> ComponentFootprint {
    let j = gullwing_large_pitch_j(level);

    // Lead pads (typically 2 or 3 small gull-wing leads on one side)
    let tol = 0.15;
    let l_min = lead_span - tol;
    let s_max = body.1 * 0.4 + tol; // approximate gap
    let w_min = lead_width - 0.05;
    let cl = 2.0 * tol;
    let cs = 2.0 * tol;
    let cw = 0.10;

    let (z, g, x) = compute_zgx(l_min, s_max, w_min, cl, cs, cw, &j);

    let pad_len = (z - g) / 2.0;
    let pad_wid = x;
    let lead_center = (z + g) / 4.0;

    let mut pads = Vec::with_capacity(pins + 1);

    // Lead pins (bottom side)
    let start_x = -(pins as f64 - 1.0) * pitch / 2.0;
    for i in 0..pins {
        let x_pos = start_x + i as f64 * pitch;
        pads.push(make_pad(
            &(pads.len() + 1).to_string(),
            x_pos,
            lead_center,
            pad_wid,
            pad_len,
        ));
    }

    // Tab/drain pad (top side, large)
    pads.push(make_pad(
        &(pads.len() + 1).to_string(),
        0.0,
        -body.1 / 4.0,
        tab.0,
        tab.1,
    ));

    ComponentFootprint {
        footprint_name: format!("DPAK_{}pin", pins),
        svg_data: String::new(),
        pad_count: pads.len() as u32,
        body_width: body.0,
        body_height: body.1,
        pitch: Some(pitch),
        pads,
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Generate an IPC-7351B compliant footprint for the given package family.
pub fn generate_footprint(family: &PackageFamily, level: DensityLevel) -> ComponentFootprint {
    match family {
        PackageFamily::Chip { l, w, t, tol } => {
            generate_chip(*l, *w, *t, *tol, level)
        }
        PackageFamily::GullWing { body, span, pitch, lead_width, pins, pins_per_side } => {
            generate_gullwing(*body, *span, *pitch, *lead_width, *pins, pins_per_side, level)
        }
        PackageFamily::QuadFlat { body, span, pitch, lead_width, pins } => {
            generate_quad(*body, *span, *pitch, *lead_width, *pins, level)
        }
        PackageFamily::QFN { body, pitch, lead_width, lead_len, pins, epad } => {
            generate_qfn(*body, *pitch, *lead_width, *lead_len, *pins, *epad, level)
        }
        PackageFamily::DPAK { body, tab, pitch, lead_width, lead_span, pins } => {
            generate_dpak(*body, *tab, *pitch, *lead_width, *lead_span, *pins, level)
        }
    }
}

/// Look up a standard package by name and return its family with JEDEC dimensions.
///
/// Package names follow common industry conventions (imperial chip sizes,
/// JEDEC outline designations).
pub fn standard_package(name: &str) -> Option<PackageFamily> {
    Some(match name {
        // ── EIA chip passives (imperial size codes) ──────────────────
        "0201" => PackageFamily::Chip { l: 0.60, w: 0.30, t: 0.15, tol: 0.05 },
        "0402" => PackageFamily::Chip { l: 1.00, w: 0.50, t: 0.25, tol: 0.05 },
        "0603" => PackageFamily::Chip { l: 1.60, w: 0.80, t: 0.30, tol: 0.10 },
        "0805" => PackageFamily::Chip { l: 2.00, w: 1.25, t: 0.40, tol: 0.10 },
        "1206" => PackageFamily::Chip { l: 3.20, w: 1.60, t: 0.50, tol: 0.15 },
        "1210" => PackageFamily::Chip { l: 3.20, w: 2.50, t: 0.50, tol: 0.15 },
        "2010" => PackageFamily::Chip { l: 5.00, w: 2.50, t: 0.50, tol: 0.15 },
        "2512" => PackageFamily::Chip { l: 6.35, w: 3.20, t: 0.50, tol: 0.15 },

        // ── SOT family (JEDEC TO-236 / TO-261) ──────────────────────
        "SOT-23" | "SOT23" => PackageFamily::GullWing {
            body: (2.90, 1.30), span: 2.40, pitch: 0.95, lead_width: 0.40,
            pins: 3, pins_per_side: vec![2, 1],
        },
        "SOT-23-5" | "SOT23-5" => PackageFamily::GullWing {
            body: (2.90, 1.60), span: 2.60, pitch: 0.95, lead_width: 0.30,
            pins: 5, pins_per_side: vec![3, 2],
        },
        "SOT-23-6" | "SOT23-6" => PackageFamily::GullWing {
            body: (2.90, 1.60), span: 2.60, pitch: 0.95, lead_width: 0.30,
            pins: 6, pins_per_side: vec![3, 3],
        },
        "SOT-223" | "SOT223" => PackageFamily::GullWing {
            body: (6.50, 3.50), span: 7.00, pitch: 2.30, lead_width: 0.70,
            pins: 4, pins_per_side: vec![3, 0, 1], // 3 leads + 1 tab
        },
        "SOT-89" | "SOT89" => PackageFamily::GullWing {
            body: (4.50, 2.50), span: 4.00, pitch: 1.50, lead_width: 0.40,
            pins: 3, pins_per_side: vec![3, 0], // 3 leads, middle is collector tab
        },

        // ── SOIC family (JEDEC MS-012) ──────────────────────────────
        "SOIC-8" | "SOP-8" | "SO-8" => PackageFamily::GullWing {
            body: (4.90, 3.90), span: 6.00, pitch: 1.27, lead_width: 0.40,
            pins: 8, pins_per_side: vec![4, 4],
        },
        "SOIC-14" | "SOP-14" | "SO-14" => PackageFamily::GullWing {
            body: (8.65, 3.90), span: 6.00, pitch: 1.27, lead_width: 0.40,
            pins: 14, pins_per_side: vec![7, 7],
        },
        "SOIC-16" | "SOP-16" | "SO-16" => PackageFamily::GullWing {
            body: (9.90, 3.90), span: 6.00, pitch: 1.27, lead_width: 0.40,
            pins: 16, pins_per_side: vec![8, 8],
        },

        // ── TSSOP (JEDEC MO-153) ────────────────────────────────────
        "TSSOP-8" => PackageFamily::GullWing {
            body: (3.00, 4.40), span: 6.40, pitch: 0.65, lead_width: 0.25,
            pins: 8, pins_per_side: vec![4, 4],
        },
        "TSSOP-14" => PackageFamily::GullWing {
            body: (5.00, 4.40), span: 6.40, pitch: 0.65, lead_width: 0.25,
            pins: 14, pins_per_side: vec![7, 7],
        },
        "TSSOP-16" => PackageFamily::GullWing {
            body: (5.00, 4.40), span: 6.40, pitch: 0.65, lead_width: 0.25,
            pins: 16, pins_per_side: vec![8, 8],
        },
        "TSSOP-20" => PackageFamily::GullWing {
            body: (6.50, 4.40), span: 6.40, pitch: 0.65, lead_width: 0.25,
            pins: 20, pins_per_side: vec![10, 10],
        },

        // ── QFP family (JEDEC MS-026) ───────────────────────────────
        "TQFP-32" => PackageFamily::QuadFlat {
            body: 7.0, span: 9.0, pitch: 0.80, lead_width: 0.37, pins: 32,
        },
        "TQFP-44" => PackageFamily::QuadFlat {
            body: 10.0, span: 12.0, pitch: 0.80, lead_width: 0.37, pins: 44,
        },
        "TQFP-48" => PackageFamily::QuadFlat {
            body: 7.0, span: 9.0, pitch: 0.50, lead_width: 0.22, pins: 48,
        },
        "TQFP-64" => PackageFamily::QuadFlat {
            body: 10.0, span: 12.0, pitch: 0.50, lead_width: 0.22, pins: 64,
        },
        "LQFP-48" => PackageFamily::QuadFlat {
            body: 7.0, span: 9.0, pitch: 0.50, lead_width: 0.22, pins: 48,
        },
        "LQFP-64" => PackageFamily::QuadFlat {
            body: 10.0, span: 12.0, pitch: 0.50, lead_width: 0.22, pins: 64,
        },
        "LQFP-100" => PackageFamily::QuadFlat {
            body: 14.0, span: 16.0, pitch: 0.50, lead_width: 0.22, pins: 100,
        },
        "LQFP-144" | "QFP-144" => PackageFamily::QuadFlat {
            body: 20.0, span: 22.0, pitch: 0.50, lead_width: 0.22, pins: 144,
        },

        // ── QFN family (JEDEC MO-220) ───────────────────────────────
        "QFN-8" | "DFN-8" => PackageFamily::QFN {
            body: 3.0, pitch: 0.50, lead_width: 0.25, lead_len: 0.40,
            pins: 8, epad: Some((1.7, 1.7)),
        },
        "QFN-16" | "DFN-16" => PackageFamily::QFN {
            body: 3.0, pitch: 0.50, lead_width: 0.25, lead_len: 0.40,
            pins: 16, epad: Some((1.7, 1.7)),
        },
        "QFN-20" => PackageFamily::QFN {
            body: 4.0, pitch: 0.50, lead_width: 0.25, lead_len: 0.40,
            pins: 20, epad: Some((2.5, 2.5)),
        },
        "QFN-24" => PackageFamily::QFN {
            body: 4.0, pitch: 0.50, lead_width: 0.25, lead_len: 0.40,
            pins: 24, epad: Some((2.5, 2.5)),
        },
        "QFN-32" => PackageFamily::QFN {
            body: 5.0, pitch: 0.50, lead_width: 0.25, lead_len: 0.40,
            pins: 32, epad: Some((3.4, 3.4)),
        },
        "QFN-48" => PackageFamily::QFN {
            body: 7.0, pitch: 0.50, lead_width: 0.25, lead_len: 0.40,
            pins: 48, epad: Some((5.2, 5.2)),
        },

        // ── DPAK / D2PAK (JEDEC TO-252 / TO-263) ───────────────────
        "DPAK" | "TO-252" => PackageFamily::DPAK {
            body: (6.60, 6.10), tab: (5.20, 5.33), pitch: 2.28,
            lead_width: 0.80, lead_span: 2.28, pins: 2,
        },
        "D2PAK" | "TO-263" => PackageFamily::DPAK {
            body: (10.30, 8.40), tab: (6.70, 8.38), pitch: 2.54,
            lead_width: 0.80, lead_span: 2.54, pins: 2,
        },

        _ => return None,
    })
}

// ── Unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chip_0603_nominal() {
        let fp = generate_footprint(
            &standard_package("0603").unwrap(),
            DensityLevel::Nominal,
        );
        assert_eq!(fp.pad_count, 2);
        assert_eq!(fp.pads.len(), 2);
        // Pads should be symmetric about origin
        assert!((fp.pads[0].x_position + fp.pads[1].x_position).abs() < 0.01);
        // Pads should have positive dimensions
        assert!(fp.pads[0].width > 0.0);
        assert!(fp.pads[0].height > 0.0);
        // Body dimensions match input
        assert!((fp.body_width - 1.6).abs() < 0.01);
        assert!((fp.body_height - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_soic8_nominal() {
        let fp = generate_footprint(
            &standard_package("SOIC-8").unwrap(),
            DensityLevel::Nominal,
        );
        assert_eq!(fp.pads.len(), 8);
        // 4 pins on left (negative x), 4 on right (positive x)
        let left: Vec<_> = fp.pads.iter().filter(|p| p.x_position < 0.0).collect();
        let right: Vec<_> = fp.pads.iter().filter(|p| p.x_position > 0.0).collect();
        assert_eq!(left.len(), 4);
        assert_eq!(right.len(), 4);
        // Pitch should be ~1.27mm
        if left.len() >= 2 {
            let dy = (left[1].y_position - left[0].y_position).abs();
            assert!((dy - 1.27).abs() < 0.1, "pitch={dy}, expected ~1.27");
        }
    }

    #[test]
    fn test_tqfp32_nominal() {
        let fp = generate_footprint(
            &standard_package("TQFP-32").unwrap(),
            DensityLevel::Nominal,
        );
        assert_eq!(fp.pads.len(), 32);
        // 8 pins per side
        let left: Vec<_> = fp.pads.iter().filter(|p| p.x_position < -3.0).collect();
        let right: Vec<_> = fp.pads.iter().filter(|p| p.x_position > 3.0).collect();
        let bottom: Vec<_> = fp.pads.iter().filter(|p| p.y_position > 3.0).collect();
        let top: Vec<_> = fp.pads.iter().filter(|p| p.y_position < -3.0).collect();
        assert_eq!(left.len(), 8);
        assert_eq!(right.len(), 8);
        assert_eq!(bottom.len(), 8);
        assert_eq!(top.len(), 8);
    }

    #[test]
    fn test_qfn32_has_epad() {
        let fp = generate_footprint(
            &standard_package("QFN-32").unwrap(),
            DensityLevel::Nominal,
        );
        assert_eq!(fp.pads.len(), 33); // 32 perimeter + 1 epad
        let epad = fp.pads.last().unwrap();
        assert!((epad.x_position).abs() < 0.01);
        assert!((epad.y_position).abs() < 0.01);
        assert!(epad.width > 3.0); // exposed pad is large
    }

    #[test]
    fn test_sot23_3pin() {
        let fp = generate_footprint(
            &standard_package("SOT-23").unwrap(),
            DensityLevel::Nominal,
        );
        assert_eq!(fp.pads.len(), 3);
        // 2 on left, 1 on right
        let left: Vec<_> = fp.pads.iter().filter(|p| p.x_position < 0.0).collect();
        let right: Vec<_> = fp.pads.iter().filter(|p| p.x_position > 0.0).collect();
        assert_eq!(left.len(), 2);
        assert_eq!(right.len(), 1);
    }

    #[test]
    fn test_all_standard_packages_valid() {
        let packages = [
            "0201", "0402", "0603", "0805", "1206", "1210", "2010", "2512",
            "SOT-23", "SOT-23-5", "SOT-23-6", "SOT-223", "SOT-89",
            "SOIC-8", "SOIC-14", "SOIC-16",
            "TSSOP-8", "TSSOP-14", "TSSOP-16", "TSSOP-20",
            "TQFP-32", "TQFP-48", "TQFP-64", "LQFP-100",
            "QFN-8", "QFN-16", "QFN-32", "QFN-48",
            "DPAK", "D2PAK",
        ];
        for name in &packages {
            let family = standard_package(name)
                .unwrap_or_else(|| panic!("standard_package({name}) returned None"));
            let fp = generate_footprint(&family, DensityLevel::Nominal);
            assert!(fp.pad_count > 0, "{name}: no pads");
            assert!(fp.body_width > 0.0, "{name}: zero body_width");
            assert!(fp.body_height > 0.0, "{name}: zero body_height");
            for pad in &fp.pads {
                assert!(pad.width > 0.0, "{name} pad {}: zero width", pad.pad_number);
                assert!(pad.height > 0.0, "{name} pad {}: zero height", pad.pad_number);
            }
        }
    }

    #[test]
    fn test_density_levels_order() {
        // Most should have larger pads than Least
        let most = generate_footprint(&standard_package("0603").unwrap(), DensityLevel::Most);
        let least = generate_footprint(&standard_package("0603").unwrap(), DensityLevel::Least);
        assert!(most.pads[0].width >= least.pads[0].width);
    }
}
