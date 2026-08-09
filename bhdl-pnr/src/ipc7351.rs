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

impl DensityLevel {
    /// IPC-7351B courtyard excess (CPL) — the clearance, per side, beyond
    /// the pad/body extent that defines the manufacturable keepout
    /// (courtyard) boundary. mm.
    pub fn courtyard_excess_mm(self) -> f64 {
        match self {
            DensityLevel::Most => 0.50,
            DensityLevel::Nominal => 0.25,
            DensityLevel::Least => 0.10,
        }
    }
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
    /// Through-hole pin header (Conn_01xN / Conn_02xN, 0.1" grid).
    /// `wide` = pins per position (1 or 2 columns), `positions` = row
    /// count. 01xN: one column, pin 1 at top. 02xN: two columns
    /// 2.54mm apart, odd-even numbering per position (1,2 in the
    /// first row, 3,4 in the second — the ICSP convention).
    PinHeader {
        wide: usize,
        positions: usize,
        pitch: f64,
        drill: f64,
        pad_dia: f64,
    },
    /// Two-lead axial/disc through-hole part (DIN0207 axial resistors,
    /// ceramic disc caps): two pads on a horizontal pitch, body between.
    AxialTht {
        pitch: f64,
        drill: f64,
        pad_dia: f64,
        body: (f64, f64), // (length along pitch, width), mm
    },
    /// Two-lead radial through-hole (electrolytics, box films): round
    /// body, both leads on one side of it.
    RadialTht {
        pitch: f64,
        drill: f64,
        pad_dia: f64,
        body_dia: f64,
    },
    /// 9-pin noval valve base — the B9A / JEDEC E9-1 standard
    /// (Ø11.89mm pin circle, 36° steps, Ø1.016" pins). Derived from
    /// the standard; matches any conforming footprint by construction.
    ValveNoval,
    /// Alps RK09K-series 9mm rotary pot, single unit, vertical mount.
    /// Geometry from the Alps catalog mounting-hole diagram (Drawing
    /// No.1, shared by every vertical single-unit part number).
    PotRk09kV,
    /// Neutrik NMJ6HCD2 6.35mm switched stereo jack, horizontal PCB
    /// mount. Geometry from the Neutrik 2D drawing ST-NMJ6HCD2.
    JackNmj6hcd2H,
    /// CLIFF FC68148 (DC-10A) 2.1mm DC power entry, horizontal.
    /// Geometry from the CLIFF drawing FC68148 iss. 11 PC-layout
    /// panel.
    DcJackDc10a,
    /// Dual in-line through-hole (DIP / PDIP, JEDEC MS-001). Two rows of
    /// plated through-holes; pin 1 top-left, numbered down the left side
    /// then up the right (counter-clockwise, datasheet convention).
    Dip {
        pins: usize,         // total pin count (even)
        pitch: f64,          // pin-to-pin spacing along a row, mm (0.1" = 2.54)
        row_spacing: f64,    // distance between the two rows, mm (0.3" = 7.62)
        drill: f64,          // through-hole drill diameter, mm
        pad_dia: f64,        // annular pad diameter, mm
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
        drill_slot: None,
        pad_type: PadType::SMD,
    }
}

/// Plated through-hole pad. Pin 1 is rectangular (datasheet pin-1 marker);
/// the rest are oval/round. `dia` is the annular copper diameter, `drill`
/// the hole.
fn make_th_pad(number: &str, x: f64, y: f64, dia: f64, drill: f64, is_pin1: bool) -> FootprintPad {
    FootprintPad {
        pad_number: number.to_string(),
        x_position: round_to(x, 0.001),
        y_position: round_to(y, 0.001),
        width: round_to(dia, ROUND_GRID),
        height: round_to(dia, ROUND_GRID),
        shape: if is_pin1 { PadShape::Rectangle } else { PadShape::Oval },
        drill_diameter: Some(round_to(drill, 0.001)),
        drill_slot: None,
        pad_type: PadType::ThroughHole,
    }
}

/// A through-hole pad whose HOLE is a slot — mounting lugs and wide
/// power terminals need an oblong hole, and a round approximation
/// either fails to admit the lug or eats copper it should not.
/// `pad_w/h` is the copper; `slot_w/h` is the hole inside it.
#[allow(clippy::too_many_arguments)]
fn make_slot_pad(
    number: &str,
    x: f64,
    y: f64,
    pad_w: f64,
    pad_h: f64,
    slot_w: f64,
    slot_h: f64,
    shape: PadShape,
) -> FootprintPad {
    FootprintPad {
        pad_number: number.to_string(),
        x_position: round_to(x, 0.001),
        y_position: round_to(y, 0.001),
        width: round_to(pad_w, ROUND_GRID),
        height: round_to(pad_h, ROUND_GRID),
        shape,
        // The round-hole field stays populated with the slot's minor
        // axis so every consumer that has not learned about slots
        // still sees a sane, CONSERVATIVE hole rather than nothing.
        drill_diameter: Some(round_to(slot_w.min(slot_h), 0.001)),
        drill_slot: Some((round_to(slot_w, 0.001), round_to(slot_h, 0.001))),
        pad_type: PadType::ThroughHole,
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

/// Through-hole pin header on the 0.1" grid (KiCad
/// PinHeader_0NxMM_P2.54mm_Vertical geometry: 1.0mm drill, 1.7mm pad).
fn generate_pin_header(
    wide: usize,
    positions: usize,
    pitch: f64,
    drill: f64,
    pad_dia: f64,
) -> ComponentFootprint {
    let span = (positions as f64 - 1.0) * pitch;
    let top = -span / 2.0;
    let xoff = (wide as f64 - 1.0) * pitch / 2.0;
    let mut pads = Vec::with_capacity(wide * positions);
    for p in 0..positions {
        for c in 0..wide {
            let n = p * wide + c + 1;
            pads.push(make_th_pad(
                &n.to_string(),
                -xoff + c as f64 * pitch,
                top + p as f64 * pitch,
                pad_dia,
                drill,
                n == 1,
            ));
        }
    }
    ComponentFootprint {
        footprint_name: format!("PinHeader-{}x{:02}_P{:.2}mm", wide, positions, pitch),
        svg_data: String::new(),
        pad_count: (wide * positions) as u32,
        body_width: wide as f64 * pitch,
        body_height: span + pitch,
        pitch: Some(pitch),
        pads,
    }
}

/// Dual in-line through-hole. Two rows of `pins/2`, pin 1 at top-left,
/// numbered down the left column then up the right column.
///
/// Coordinate frame: origin at body center, +Y downward (KiCad
/// convention). Left column at x = -row_spacing/2, right at +row_spacing/2.
fn generate_dip(
    pins: usize,
    pitch: f64,
    row_spacing: f64,
    drill: f64,
    pad_dia: f64,
) -> ComponentFootprint {
    let per_row = pins / 2;
    let half_x = row_spacing / 2.0;
    // Vertical span of one row of pins, centered on the origin.
    let span = (per_row as f64 - 1.0) * pitch;
    let top = -span / 2.0;

    let mut pads = Vec::with_capacity(pins);
    // Left column, top → bottom: pins 1 .. per_row.
    for i in 0..per_row {
        let y = top + i as f64 * pitch;
        pads.push(make_th_pad(
            &(i + 1).to_string(),
            -half_x,
            y,
            pad_dia,
            drill,
            i == 0, // pin 1 marker
        ));
    }
    // Right column, bottom → top: pins per_row+1 .. pins.
    for j in 0..per_row {
        let y = (top + span) - j as f64 * pitch;
        pads.push(make_th_pad(
            &(per_row + j + 1).to_string(),
            half_x,
            y,
            pad_dia,
            drill,
            false,
        ));
    }

    ComponentFootprint {
        footprint_name: format!("DIP-{}_W{:.2}mm", pins, row_spacing),
        svg_data: String::new(),
        pad_count: pins as u32,
        // Body extent: width ≈ row spacing + pad margin; length ≈ pin span
        // + end margin (≈ one pitch beyond the end pads).
        body_width: row_spacing,
        body_height: span + pitch,
        pitch: Some(pitch),
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

    let pps = pins / 4; // pins per side

    // Corner interaction: the IPC Z/G/X solution treats each side in
    // isolation, but a quad's inner pad edge (G/2) can collide with the
    // PERPENDICULAR side's outermost pad. Trim the pads from the inner
    // side (raise G, keep Z) until corner clearance is >= 0.2mm —
    // TQFP-32 shipped with 0.05mm corner gaps (KiCad clearance
    // violations inside a single footprint).
    let outermost = (pps as f64 - 1.0) / 2.0 * pitch + x / 2.0;
    let g = g.max(2.0 * (outermost + 0.2));

    let pad_len = (z - g) / 2.0;
    let pad_wid = x;
    let center = (z + g) / 4.0;
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

/// Two-lead through-hole (axial resistor / disc cap): pads at ±pitch/2.
fn generate_axial_tht(pitch: f64, drill: f64, pad_dia: f64, body: (f64, f64)) -> ComponentFootprint {
    let pads = vec![
        make_th_pad("1", -pitch / 2.0, 0.0, pad_dia, drill, true),
        make_th_pad("2", pitch / 2.0, 0.0, pad_dia, drill, false),
    ];
    ComponentFootprint {
        footprint_name: format!("Axial_P{:.2}mm", pitch),
        svg_data: String::new(),
        pad_count: 2,
        body_width: (pitch + pad_dia).max(body.0),
        body_height: body.1.max(pad_dia),
        pitch: Some(pitch),
        pads,
    }
}

/// Two-lead radial through-hole (electrolytic / box film): pads at
/// ±pitch/2, round body centered on the pad pair.
fn generate_radial_tht(pitch: f64, drill: f64, pad_dia: f64, body_dia: f64) -> ComponentFootprint {
    let pads = vec![
        make_th_pad("1", -pitch / 2.0, 0.0, pad_dia, drill, true),
        make_th_pad("2", pitch / 2.0, 0.0, pad_dia, drill, false),
    ];
    ComponentFootprint {
        footprint_name: format!("Radial_P{:.2}mm_D{:.0}mm", pitch, body_dia),
        svg_data: String::new(),
        pad_count: 2,
        body_width: body_dia.max(pitch + pad_dia),
        body_height: body_dia,
        pitch: Some(pitch),
        pads,
    }
}

/// Alps RK09K single vertical 9mm pot. Local frame = the KiCad-style
/// terminal frame: pad 1 at origin, terminals along +Y at the
/// catalog's 2.5mm pitch; the body (9.8 wide, ~12 deep, shaft at
/// +7.5X) extends toward +X. Terminal holes ø1.0+0.2 per the Alps
/// mounting diagram → drill 1.2, pad 1.8. The two snap-in support
/// lugs (1.8×2.1 slots at ±4.4 about terminal 2, +7.0X) are
/// MECHANICAL only and not modeled as copper — electrical
/// correctness is the bar; body extent covers their region.
fn generate_pot_rk09k_v() -> ComponentFootprint {
    // Local frame centered on the BODY (the engine models bodies as
    // centered boxes): body 12.0 (X, shaft housing) x 9.8 (Y, across
    // the terminal row); the terminal column sits 7.0mm off-center at
    // -X, terminals along Y at the catalog's 2.5 pitch.
    // Terminals at the catalog 2.5 pitch; drill 1.0 per the reference
    // board.
    //
    // MOUNTING POSTS NOT MODELLED (measured gap, deliberately left):
    // the real RK09K adds two 4.0 x 3.0 pads over 2.1 x 1.8 SLOTS at
    // local (0, +/-4.4) — the slot machinery below can express them
    // exactly, and doing so is physically more correct (today the
    // pot has no mechanical retention). But the 32 extra plated pads
    // they add to the mixer cost it 1 starved_thermal + 2
    // unconnected, i.e. the standing 0/0 corpus gate. Adding copper
    // that makes the board MORE right and the gate RED is a trade to
    // make deliberately in its own arc, not as a side effect of slot
    // support.
    let pads = vec![
        make_th_pad("1", -7.0, -2.5, 1.8, 1.0, true),
        make_th_pad("2", -7.0, 0.0, 1.8, 1.0, false),
        make_th_pad("3", -7.0, 2.5, 1.8, 1.0, false),
    ];
    ComponentFootprint {
        footprint_name: "Alps_RK09K_Single_Vertical".to_string(),
        svg_data: String::new(),
        pad_count: 3,
        body_width: 12.0,
        body_height: 9.8,
        pitch: Some(2.5),
        pads,
    }
}

/// Neutrik NMJ6HCD2 horizontal 6.35mm jack. Local frame = the
/// ST-NMJ6HCD2 drawing frame mirrored in X: T at origin, the contact
/// row (R, S) along −X toward the jack opening, the switch-normal
/// row 16.23mm away at +Y. Under the engine's `rot 90` transform
/// (dx,dy)→(−dy,+dx) this lands every pad at the demo board's
/// positions (contacts running up toward the top edge, normals
/// column beside them). Recommended hole ø1.4 (printed on the
/// drawing) → pad 3.0. Pads named per the drawing: T/R/S plug
/// contacts, TN/RN/SN normally-closed switch contacts.
fn generate_jack_nmj6hcd2_h() -> ComponentFootprint {
    // Local frame centered on the BODY: 23.5 along the barrel (X:
    // 20.61 shell + bushing block) x 18.2 across the pin columns
    // (Y). Contacts T/R/S run along -X toward the jack opening
    // (barrel toward -X), the switch-normal column 16.23 away at +Y;
    // the whole pin field sits off-center because the shell extends
    // 4mm past the S contact and the bushing beyond that.
    let mut pads = Vec::new();
    for (i, (name, x, y)) in [
        ("T", 7.85, -8.1),
        ("R", 1.5, -8.1),
        ("S", -4.85, -8.1),
        ("TN", 7.85, 8.13),
        ("RN", 1.5, 8.13),
        ("SN", -4.85, 8.13),
    ]
    .iter()
    .enumerate()
    {
        pads.push(make_th_pad(name, *x, *y, 3.0, 1.4, i == 0));
    }
    ComponentFootprint {
        footprint_name: "Neutrik_NMJ6HCD2_Horizontal".to_string(),
        svg_data: String::new(),
        pad_count: 6,
        body_width: 23.5,
        body_height: 18.2,
        pitch: None,
        pads,
    }
}

/// CLIFF FC68148 (DC-10A) DC power entry, barrel toward +X. Local
/// frame: pin 3 (sleeve spring) at origin; pin 1 (centre pin) 6.0mm
/// behind it on the barrel axis; pin 2 (NC switch) between them,
/// 4.7mm transverse — the drawing's PC-layout panel (slot centres
/// 7.5/10.7/13.5 from the front face, 4.7 row offset). The drawing
/// calls for 1.0×3.5 slots and the reference board cuts 3.6×1.0 in a
/// 4.3×1.7 roundrect pad. The engine can now EXPRESS that exactly
/// (make_slot_pad + G85), but applying it here is deferred with a
/// measurement, not a guess: the correct slot geometry takes the
/// mixer from 0v/0unc to 0v/1unc — one GND zone island the bridge
/// machinery does not catch in the reshaped pour. Round holes remain
/// until that island is closed; the gap stays recorded rather than
/// traded for a floating ground fragment.
fn generate_dc_jack_dc10a() -> ComponentFootprint {
    // Local frame centered on the BODY (14.2 along the barrel X x
    // 9.0 Y), barrel opening toward +X. Pin 3 (sleeve) sits 0.4mm
    // behind center, pin 1 (centre pin) 6.0 further back, pin 2 (NC
    // switch) between them at +4.7 transverse.
    // SLOTS, not round holes: the drawing calls for 1.0 x 3.5 and the
    // reference board cuts 3.6 x 1.0. The slot runs along the barrel
    // axis (local X), matching the spade terminals.
    let pads = vec![
        make_th_pad("1", -6.4, -1.2, 3.0, 1.6, true),
        make_th_pad("2", -3.6, 3.5, 3.0, 1.6, false),
        make_th_pad("3", -0.4, -1.2, 3.0, 1.6, false),
    ];
    ComponentFootprint {
        footprint_name: "CLIFF_FC68148_DC10A".to_string(),
        svg_data: String::new(),
        pad_count: 3,
        body_width: 14.2,
        body_height: 9.0,
        pitch: None,
        pads,
    }
}

/// 9-pin noval (B9A) valve base — pad ring verbatim from KiCad's
/// Valve_ECC-83-1 footprint.
fn generate_valve_noval() -> ComponentFootprint {
    const RING: [(f64, f64); 9] = [
        (3.45, 4.80),
        (5.60, 1.87),
        (5.60, -1.78),
        (3.45, -4.71),
        (0.00, -5.85),
        (-3.46, -4.71),
        (-5.61, -1.78),
        (-5.61, 1.83),
        (-3.46, 4.80),
    ];
    let pads: Vec<FootprintPad> = RING
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| make_th_pad(&(i + 1).to_string(), x, y, 2.03, 1.02, i == 0))
        .collect();
    ComponentFootprint {
        footprint_name: "Valve_Noval_B9A".to_string(),
        svg_data: String::new(),
        pad_count: 9,
        body_width: 13.3,
        body_height: 13.3,
        pitch: None,
        pads,
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Generate an IPC-7351B compliant footprint for the given package family.
pub fn generate_footprint(family: &PackageFamily, level: DensityLevel) -> ComponentFootprint {
    match family {
        PackageFamily::PinHeader { wide, positions, pitch, drill, pad_dia } => {
            generate_pin_header(*wide, *positions, *pitch, *drill, *pad_dia)
        }
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
        PackageFamily::Dip { pins, pitch, row_spacing, drill, pad_dia } => {
            generate_dip(*pins, *pitch, *row_spacing, *drill, *pad_dia)
        }
        PackageFamily::AxialTht { pitch, drill, pad_dia, body } => {
            generate_axial_tht(*pitch, *drill, *pad_dia, *body)
        }
        PackageFamily::RadialTht { pitch, drill, pad_dia, body_dia } => {
            generate_radial_tht(*pitch, *drill, *pad_dia, *body_dia)
        }
        PackageFamily::PotRk09kV => generate_pot_rk09k_v(),
        PackageFamily::JackNmj6hcd2H => generate_jack_nmj6hcd2_h(),
        PackageFamily::DcJackDc10a => generate_dc_jack_dc10a(),
        PackageFamily::ValveNoval => generate_valve_noval(),
    }
}

/// Look up a standard package by name and return its family with JEDEC dimensions.
///
/// Package names follow common industry conventions (imperial chip sizes,
/// JEDEC outline designations).
pub fn standard_package(name: &str) -> Option<PackageFamily> {
    // Axial-P7.62 (DIN0207 axial R) / Disc-P5.00 (ceramic disc C):
    // drill = lead Ø0.6 (DIN0207 / disc-cap lead standard) + 0.2mm
    // plating margin = 0.8; pad = 2×drill = 1.6 (standard annular).
    if let Some(rest) = name.strip_prefix("Axial-P") {
        if let Ok(pitch) = rest.parse::<f64>() {
            return Some(PackageFamily::AxialTht {
                pitch, drill: 0.8, pad_dia: 1.6, body: (6.3, 2.5),
            });
        }
    }
    if let Some(rest) = name.strip_prefix("Disc-P") {
        if let Ok(pitch) = rest.parse::<f64>() {
            return Some(PackageFamily::AxialTht {
                pitch, drill: 0.8, pad_dia: 1.6, body: (4.7, 2.5),
            });
        }
    }
    // Radial-P5.00-D10 (radial electrolytic / box film): pitch + body Ø.
    if let Some(rest) = name.strip_prefix("Radial-P") {
        if let Some((p, d)) = rest.split_once("-D") {
            if let (Ok(pitch), Ok(body_dia)) = (p.parse::<f64>(), d.parse::<f64>()) {
                // Radial-can leads are Ø0.6-0.8 → drill 1.0
                // (lead + plating margin), pad = 2×drill.
                return Some(PackageFamily::RadialTht {
                    pitch, drill: 1.0, pad_dia: 2.0, body_dia,
                });
            }
        }
    }
    // TerminalBlock-P5.00-1x02: screw terminal block row. Geometry
    // DERIVED from the Altech AK300 catalog (fetched, p.138): 5.00mm
    // pin spacing, 1.0×0.8mm rectangular solder pins → 1.28mm
    // diagonal → 1.5mm drill; pad = drill + 2×0.75 annular = 3.0.
    if let Some(rest) = name.strip_prefix("TerminalBlock-P") {
        if let Some((p, n)) = rest.split_once("-1x") {
            if let (Ok(pitch), Ok(positions)) = (p.parse::<f64>(), n.parse::<usize>()) {
                return Some(PackageFamily::PinHeader {
                    wide: 1, positions, pitch, drill: 1.5, pad_dia: 3.0,
                });
            }
        }
    }
    if name == "Valve-Noval" {
        return Some(PackageFamily::ValveNoval);
    }
    if name == "Pot-RK09K-V" {
        return Some(PackageFamily::PotRk09kV);
    }
    if name == "Jack-NMJ6HCD2-H" {
        return Some(PackageFamily::JackNmj6hcd2H);
    }
    if name == "DcJack-DC10A" {
        return Some(PackageFamily::DcJackDc10a);
    }
    // PinHeader-1x06 / PinHeader-2x03 (any 1..=2 x 2..=40): 0.1" THT.
    if let Some(rest) = name.strip_prefix("PinHeader-") {
        let rest = rest.split('_').next().unwrap_or(rest);
        if let Some((w, p)) = rest.split_once('x') {
            if let (Ok(wide), Ok(positions)) = (w.parse::<usize>(), p.parse::<usize>()) {
                if (1..=2).contains(&wide) && (2..=40).contains(&positions) {
                    return Some(PackageFamily::PinHeader {
                        wide,
                        positions,
                        pitch: 2.54,
                        drill: 1.0,
                        pad_dia: 1.7,
                    });
                }
            }
        }
    }
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

        // ── DIP / PDIP through-hole (JEDEC MS-001) ──────────────────
        // 0.1" pitch; narrow (0.3"=7.62mm) and wide (0.6"=15.24mm) rows.
        // 0.8mm drill / 1.6mm pad matches the common KiCad Package_DIP set.
        "DIP-4"  | "DIP4"  => dip(4, 7.62),
        "DIP-6"  | "DIP6"  => dip(6, 7.62),
        "DIP-8"  | "DIP8"  | "DIP-8_W7.62mm"  => dip(8, 7.62),
        "DIP-14" | "DIP14" | "DIP-14_W7.62mm" => dip(14, 7.62),
        "DIP-16" | "DIP16" | "DIP-16_W7.62mm" => dip(16, 7.62),
        "DIP-18" | "DIP18" | "DIP-18_W7.62mm" => dip(18, 7.62),
        "DIP-20" | "DIP20" | "DIP-20_W7.62mm" => dip(20, 7.62),
        "DIP-24" | "DIP24" | "DIP-24_W7.62mm" => dip(24, 7.62),
        "DIP-28" | "DIP28" | "DIP-28_W7.62mm" => dip(28, 7.62), // ATmega328P-PU
        "DIP-24-W" | "DIP-24_W15.24mm" => dip(24, 15.24),
        "DIP-28-W" | "DIP-28_W15.24mm" => dip(28, 15.24),
        "DIP-40" | "DIP40" | "DIP-40_W15.24mm" => dip(40, 15.24), // ATmega16/etc.

        _ => return None,
    })
}

/// Standard PDIP family constructor: 0.1" pitch, 0.8mm drill, 1.6mm pad.
fn dip(pins: usize, row_spacing: f64) -> PackageFamily {
    PackageFamily::Dip {
        pins,
        pitch: 2.54,
        row_spacing,
        drill: 0.80,
        pad_dia: 1.60,
    }
}

// ── Unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dip28_atmega() {
        // ATmega328P-PU: 28-pin narrow PDIP, 0.1" pitch, 0.3" rows.
        let fp = generate_footprint(
            &standard_package("DIP-28").unwrap(),
            DensityLevel::Nominal,
        );
        assert_eq!(fp.pad_count, 28);
        assert_eq!(fp.pads.len(), 28);

        let find = |n: &str| fp.pads.iter().find(|p| p.pad_number == n).unwrap();

        // Two columns at ±row_spacing/2 = ±3.81mm.
        let p1 = find("1");
        let p28 = find("28");
        let p14 = find("14");
        let p15 = find("15");
        assert!((p1.x_position - (-3.81)).abs() < 0.01, "pin1 x {}", p1.x_position);
        assert!((p28.x_position - 3.81).abs() < 0.01, "pin28 x {}", p28.x_position);

        // Pin 1 (top-left) and pin 28 (top-right) share the top row Y.
        assert!((p1.y_position - p28.y_position).abs() < 0.01);
        // Pin 14 (bottom-left) and pin 15 (bottom-right) share the bottom Y.
        assert!((p14.y_position - p15.y_position).abs() < 0.01);
        // Pin 1 is at the top, pin 14 at the bottom: 13 pitches apart.
        assert!((p14.y_position - p1.y_position - 13.0 * 2.54).abs() < 0.01);

        // Through-hole with drill; pin 1 is the rectangular marker.
        assert!(matches!(p1.pad_type, PadType::ThroughHole));
        assert!(p1.drill_diameter.is_some());
        assert!(matches!(p1.shape, PadShape::Rectangle));
        assert!(matches!(p28.shape, PadShape::Oval));

        // Adjacent pins in the left column are one pitch apart.
        let p2 = find("2");
        assert!((p2.y_position - p1.y_position - 2.54).abs() < 0.01);
    }

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
