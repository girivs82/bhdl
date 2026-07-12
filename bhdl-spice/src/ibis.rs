//! IBIS (.ibs) model file parser — the vendor I/O-buffer format
//! (Vendor_Simulation_Blocks.md §5, vendor-model form #1).
//!
//! Scope: the subset GLACIER's DC/transient stamping consumes —
//! `[Component]` / `[Pin]` (pin → signal/model mapping), `[Model Selector]`,
//! and per-`[Model]`: `Model_type`, `[Voltage Range]`, `C_comp`, the four
//! I-V tables (`[Pulldown]`, `[Pullup]`, `[GND Clamp]`, `[POWER Clamp]`)
//! with typ/min/max columns, and `[Ramp]`. Keywords we don't consume
//! (waveforms, submodels, package parasitics) are skipped structurally, so
//! any spec-conformant file parses.
//!
//! Two correctness details the format hides:
//! - **Pullup and POWER-clamp table voltages are Vcc-relative**: the V
//!   column is `Vcc − Vpin`, not Vpin. [`Model::iv_at`] does the flip so
//!   callers always work in pin volts.
//! - `NA` entries are legal anywhere a number is; a column that is NA at a
//!   given row simply contributes no point for that corner.
//!
//! Real-Data: this parser never invents values — a missing table is
//! `None`, a missing corner column yields no points, and callers degrade
//! per the §5 ladder rather than substituting.

use std::collections::HashMap;
use std::path::Path;

/// Which datasheet corner to read from the three-column tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Corner {
    #[default]
    Typ,
    Min,
    Max,
}

impl Corner {
    pub fn parse(s: &str) -> Option<Corner> {
        match s.to_ascii_lowercase().as_str() {
            "typ" | "typical" => Some(Corner::Typ),
            "min" | "slow" | "weak" => Some(Corner::Min),
            "max" | "fast" | "strong" => Some(Corner::Max),
            _ => None,
        }
    }
}

/// One I-V table: per-corner point lists in PIN volts (Vcc-relative
/// source tables are flipped at load time by the caller — see
/// [`Model::iv_at`] which flips at evaluation instead, keeping the raw
/// table faithful to the file).
#[derive(Debug, Clone, Default)]
pub struct IvTable {
    /// (voltage, current) points as written in the file, per corner.
    pub typ: Vec<(f64, f64)>,
    pub min: Vec<(f64, f64)>,
    pub max: Vec<(f64, f64)>,
}

impl IvTable {
    pub fn points(&self, corner: Corner) -> &[(f64, f64)] {
        match corner {
            Corner::Typ => &self.typ,
            Corner::Min => &self.min,
            Corner::Max => &self.max,
        }
    }

    /// Piecewise-linear interpolation at `v` (file coordinates), clamped
    /// to the table's end currents outside its range. None if the corner
    /// has no points (all-NA column) — absence, never a guess.
    pub fn interpolate(&self, corner: Corner, v: f64) -> Option<f64> {
        let pts = self.points(corner);
        if pts.is_empty() {
            return None;
        }
        if v <= pts[0].0 {
            return Some(pts[0].1);
        }
        if v >= pts[pts.len() - 1].0 {
            return Some(pts[pts.len() - 1].1);
        }
        for w in pts.windows(2) {
            let (v0, i0) = w[0];
            let (v1, i1) = w[1];
            if v <= v1 {
                let t = (v - v0) / (v1 - v0);
                return Some(i0 + t * (i1 - i0));
            }
        }
        Some(pts[pts.len() - 1].1)
    }
}

/// `[Ramp]` — dV/dt over the 20–80% swing, per corner, as (dv, dt).
#[derive(Debug, Clone, Default)]
pub struct Ramp {
    pub dv_dt_r: [Option<(f64, f64)>; 3], // [typ, min, max]
    pub dv_dt_f: [Option<(f64, f64)>; 3],
}

/// One `[Model]` section.
#[derive(Debug, Clone, Default)]
pub struct Model {
    pub name: String,
    /// `Model_type` verbatim, lowercased: "input", "output", "i/o",
    /// "3-state", "open_drain", "open_sink", "open_source", …
    pub model_type: String,
    /// `[Voltage Range]` typ/min/max (the buffer's Vcc), volts.
    pub voltage_range: [Option<f64>; 3],
    /// `C_comp` typ/min/max, farads.
    pub c_comp: [Option<f64>; 3],
    pub pulldown: Option<IvTable>,
    pub pullup: Option<IvTable>,
    pub gnd_clamp: Option<IvTable>,
    pub power_clamp: Option<IvTable>,
    pub ramp: Ramp,
}

impl Model {
    /// The buffer's supply voltage for `corner` (falls back to typ —
    /// the file's own declaration, not an invented number).
    pub fn vcc(&self, corner: Corner) -> Option<f64> {
        let idx = match corner {
            Corner::Typ => 0,
            Corner::Min => 1,
            Corner::Max => 2,
        };
        self.voltage_range[idx].or(self.voltage_range[0])
    }

    /// Element current at PIN voltage `v_pin`, for one of the four
    /// elements. Handles the Vcc-relative coordinate flip for the
    /// pullup/POWER-clamp tables (`V_table = Vcc − V_pin`). Returns None
    /// when the element or its corner data is absent.
    pub fn iv_at(&self, element: IvElement, corner: Corner, v_pin: f64) -> Option<f64> {
        let (table, vcc_relative) = match element {
            IvElement::Pulldown => (self.pulldown.as_ref()?, false),
            IvElement::GndClamp => (self.gnd_clamp.as_ref()?, false),
            IvElement::Pullup => (self.pullup.as_ref()?, true),
            IvElement::PowerClamp => (self.power_clamp.as_ref()?, true),
        };
        let v_table = if vcc_relative {
            self.vcc(corner)? - v_pin
        } else {
            v_pin
        };
        table.interpolate(corner, v_table)
    }
}

/// Buffer logic state for DC composition: which drive elements conduct.
/// Clamps are always present; `High` adds the pullup, `Low` the pulldown,
/// `HiZ` (and input-type models) clamps only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferState {
    High,
    Low,
    #[default]
    HiZ,
}

impl BufferState {
    pub fn parse(s: &str) -> Option<BufferState> {
        match s.to_ascii_lowercase().as_str() {
            "high" | "1" | "hi" => Some(BufferState::High),
            "low" | "0" | "lo" => Some(BufferState::Low),
            "hiz" | "hi-z" | "z" | "off" | "input" => Some(BufferState::HiZ),
            _ => None,
        }
    }
}

impl Model {
    /// Compose the buffer's total DC I-V into one piecewise-linear table
    /// in PIN volts: the sum of the state-active drive element and both
    /// clamps, sampled at the union of every contributing table's
    /// breakpoints (Vcc-relative tables flipped). Current follows the
    /// branch convention `i(v_pin)` = current flowing from the pin into
    /// the buffer (IBIS's own sign: positive = sinking).
    ///
    /// Returns None when nothing conducts (no tables for this state) —
    /// absence, not a zero-current fabrication.
    pub fn composed_iv(
        &self,
        state: BufferState,
        corner: Corner,
    ) -> Option<Vec<(f64, f64)>> {
        let mut elements: Vec<IvElement> = vec![IvElement::GndClamp, IvElement::PowerClamp];
        match state {
            BufferState::High => elements.push(IvElement::Pullup),
            BufferState::Low => elements.push(IvElement::Pulldown),
            BufferState::HiZ => {}
        }

        // Union of breakpoints, in pin volts.
        let mut breakpoints: Vec<f64> = Vec::new();
        let mut any = false;
        for el in &elements {
            let (table, vcc_relative) = match el {
                IvElement::Pulldown => (self.pulldown.as_ref(), false),
                IvElement::GndClamp => (self.gnd_clamp.as_ref(), false),
                IvElement::Pullup => (self.pullup.as_ref(), true),
                IvElement::PowerClamp => (self.power_clamp.as_ref(), true),
            };
            let Some(table) = table else { continue };
            let pts = table.points(corner);
            if pts.is_empty() {
                continue;
            }
            any = true;
            for (v, _) in pts {
                let v_pin = if vcc_relative { self.vcc(corner)? - v } else { *v };
                breakpoints.push(v_pin);
            }
        }
        if !any {
            return None;
        }
        breakpoints.sort_by(|a, b| a.partial_cmp(b).unwrap());
        breakpoints.dedup_by(|a, b| (*a - *b).abs() < 1e-12);

        let pts: Vec<(f64, f64)> = breakpoints
            .into_iter()
            .map(|v_pin| {
                let i: f64 = elements
                    .iter()
                    .filter_map(|el| self.iv_at(*el, corner, v_pin))
                    .sum();
                (v_pin, i)
            })
            .collect();
        Some(pts)
    }
}

/// The four I-V elements of an IBIS buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IvElement {
    Pulldown,
    Pullup,
    GndClamp,
    PowerClamp,
}

/// One `[Pin]` row of a `[Component]`.
#[derive(Debug, Clone)]
pub struct Pin {
    pub pin: String,         // package pin number/name ("29", "A4")
    pub signal_name: String, // vendor's signal name ("PD2", "DQ0")
    pub model_name: String,  // model or model-selector name; "NC"/"GND"/"POWER" specials
}

/// One `[Component]` section.
#[derive(Debug, Clone, Default)]
pub struct Component {
    pub name: String,
    pub manufacturer: String,
    pub pins: Vec<Pin>,
}

impl Component {
    /// Find the pin row for an entity pin: by signal name first
    /// (case-insensitive), then by package pin number.
    pub fn pin_for(&self, name: &str) -> Option<&Pin> {
        self.pins
            .iter()
            .find(|p| p.signal_name.eq_ignore_ascii_case(name))
            .or_else(|| self.pins.iter().find(|p| p.pin.eq_ignore_ascii_case(name)))
    }
}

/// A parsed .ibs file.
#[derive(Debug, Clone, Default)]
pub struct IbisFile {
    pub ibis_ver: String,
    pub file_name: String,
    pub components: Vec<Component>,
    pub models: HashMap<String, Model>,
    /// `[Model Selector]` name → first listed model name (the IBIS
    /// default selection; explicit selection is a later surface).
    pub model_selectors: HashMap<String, String>,
}

impl IbisFile {
    pub fn component(&self, name: &str) -> Option<&Component> {
        self.components
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// Resolve a `[Pin]` model reference through `[Model Selector]`
    /// indirection to a concrete model. None for the NC/GND/POWER
    /// specials and unknown names.
    pub fn resolve_model(&self, model_name: &str) -> Option<&Model> {
        if matches!(model_name.to_ascii_uppercase().as_str(), "NC" | "GND" | "POWER") {
            return None;
        }
        if let Some(m) = self.models.get(model_name) {
            return Some(m);
        }
        self.model_selectors
            .get(model_name)
            .and_then(|first| self.models.get(first))
    }
}

/// Parse error with the 1-based line it occurred on.
#[derive(Debug)]
pub struct IbisError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for IbisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ibis parse error at line {}: {}", self.line, self.message)
    }
}
impl std::error::Error for IbisError {}

pub fn parse_file(path: &Path) -> Result<IbisFile, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    parse_str(&text).map_err(Into::into)
}

/// Parse IBIS text. Line-oriented: `[Keyword] arg` headers, `|` comments,
/// whitespace-separated data rows. Unknown keywords are skipped until the
/// next `[` header, so conformant files with sections we don't model
/// still parse.
pub fn parse_str(text: &str) -> Result<IbisFile, IbisError> {
    let mut out = IbisFile::default();
    let mut cur_component: Option<Component> = None;
    let mut cur_model: Option<Model> = None;
    // What the current data rows belong to.
    #[derive(PartialEq)]
    enum Section {
        None,
        Pins,
        Iv(IvElement),
        Ramp,
        ModelSelector(String),
        VoltageRange,
    }
    let mut section = Section::None;

    // Number with IBIS engineering suffixes (4.0m, 2.2k, 10.0u, 1n, NA).
    fn num(tok: &str) -> Option<f64> {
        let t = tok.trim();
        if t.eq_ignore_ascii_case("na") {
            return None;
        }
        // Split trailing unit letters; first char of the suffix may be an
        // SI prefix (IBIS allows e.g. "3.5mA", "500.0m", "1.2V").
        let split = t
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E'))
            .unwrap_or(t.len());
        // Guard: 'e'/'E' inside an exponent was excluded above only when
        // followed by digits — re-split conservatively.
        let (mantissa, suffix) = t.split_at(split);
        let base: f64 = mantissa.parse().ok()?;
        let scale = match suffix.chars().next() {
            None => 1.0,
            Some('T') => 1e12,
            Some('G') => 1e9,
            Some('M') => 1e6,
            Some('k') | Some('K') => 1e3,
            Some('m') => 1e-3,
            Some('u') | Some('µ') => 1e-6,
            Some('n') => 1e-9,
            Some('p') => 1e-12,
            Some('f') => 1e-15,
            // A bare unit letter (V, A, F, s, Ohm…) with no prefix.
            Some(_) => 1.0,
        };
        Some(base * scale)
    }

    fn flush_model(out: &mut IbisFile, m: Option<Model>) {
        if let Some(m) = m {
            out.models.insert(m.name.clone(), m);
        }
    }
    fn flush_component(out: &mut IbisFile, c: Option<Component>) {
        if let Some(c) = c {
            out.components.push(c);
        }
    }

    for (idx, raw) in text.lines().enumerate() {
        let lineno = idx + 1;
        // `|` starts a comment (IBIS default comment char).
        let line = match raw.find('|') {
            Some(i) => &raw[..i],
            None => raw,
        };
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }

        if let Some(rest) = line.trim_start().strip_prefix('[') {
            let Some(close) = rest.find(']') else {
                return Err(IbisError { line: lineno, message: "unterminated [keyword]".into() });
            };
            let keyword = rest[..close].trim().to_ascii_lowercase();
            let arg = rest[close + 1..].trim().to_string();
            section = Section::None;
            match keyword.as_str() {
                "ibis ver" => out.ibis_ver = arg,
                "file name" => out.file_name = arg,
                "component" => {
                    flush_component(&mut out, cur_component.take());
                    cur_component = Some(Component { name: arg, ..Default::default() });
                }
                "manufacturer" => {
                    if let Some(c) = cur_component.as_mut() {
                        c.manufacturer = arg;
                    }
                }
                "pin" => section = Section::Pins,
                "model selector" => section = Section::ModelSelector(arg),
                "model" => {
                    flush_model(&mut out, cur_model.take());
                    cur_model = Some(Model { name: arg, ..Default::default() });
                }
                "voltage range" => {
                    // Values may sit on the keyword line or the next row.
                    let toks: Vec<&str> = arg.split_whitespace().collect();
                    if toks.is_empty() {
                        section = Section::VoltageRange;
                    } else if let Some(m) = cur_model.as_mut() {
                        for (i, t) in toks.iter().take(3).enumerate() {
                            m.voltage_range[i] = num(t);
                        }
                    }
                }
                "pulldown" => section = Section::Iv(IvElement::Pulldown),
                "pullup" => section = Section::Iv(IvElement::Pullup),
                "gnd clamp" => section = Section::Iv(IvElement::GndClamp),
                "power clamp" => section = Section::Iv(IvElement::PowerClamp),
                "ramp" => section = Section::Ramp,
                "end" => break,
                _ => { /* unmodeled section — data rows fall through Section::None and are skipped */ }
            }
            continue;
        }

        // Sub-parameter lines inside a [Model] (`Model_type Output`,
        // `C_comp 3.5pF 3.0pF 4.0pF`) and data rows.
        let toks: Vec<&str> = line.split_whitespace().collect();
        match &section {
            Section::Pins => {
                if let Some(c) = cur_component.as_mut() {
                    // Header row "signal_name model_name ..." is literal in
                    // many files — skip it by name.
                    if toks.len() >= 3 && !toks[1].eq_ignore_ascii_case("signal_name") {
                        c.pins.push(Pin {
                            pin: toks[0].to_string(),
                            signal_name: toks[1].to_string(),
                            model_name: toks[2].to_string(),
                        });
                    }
                }
            }
            Section::ModelSelector(name) => {
                // First data row = default selection.
                if !toks.is_empty() && !out.model_selectors.contains_key(name) {
                    out.model_selectors.insert(name.clone(), toks[0].to_string());
                }
            }
            Section::VoltageRange => {
                if let Some(m) = cur_model.as_mut() {
                    for (i, t) in toks.iter().take(3).enumerate() {
                        m.voltage_range[i] = num(t);
                    }
                }
                section = Section::None;
            }
            Section::Iv(el) => {
                if let Some(m) = cur_model.as_mut() {
                    if toks.len() >= 2 {
                        if let Some(v) = num(toks[0]) {
                            let table = match el {
                                IvElement::Pulldown => m.pulldown.get_or_insert_with(Default::default),
                                IvElement::Pullup => m.pullup.get_or_insert_with(Default::default),
                                IvElement::GndClamp => m.gnd_clamp.get_or_insert_with(Default::default),
                                IvElement::PowerClamp => m.power_clamp.get_or_insert_with(Default::default),
                            };
                            if let Some(i) = toks.get(1).and_then(|t| num(t)) {
                                table.typ.push((v, i));
                            }
                            if let Some(i) = toks.get(2).and_then(|t| num(t)) {
                                table.min.push((v, i));
                            }
                            if let Some(i) = toks.get(3).and_then(|t| num(t)) {
                                table.max.push((v, i));
                            }
                        }
                    }
                }
            }
            Section::Ramp => {
                if let Some(m) = cur_model.as_mut() {
                    // dV/dt_r 2.0/1.0n 1.5/1.2n 2.5/0.8n
                    let parse_ratio = |t: &str| -> Option<(f64, f64)> {
                        let (a, b) = t.split_once('/')?;
                        Some((num(a)?, num(b)?))
                    };
                    if toks.first().map(|t| t.eq_ignore_ascii_case("dv/dt_r")).unwrap_or(false) {
                        for (i, t) in toks.iter().skip(1).take(3).enumerate() {
                            m.ramp.dv_dt_r[i] = parse_ratio(t);
                        }
                    } else if toks.first().map(|t| t.eq_ignore_ascii_case("dv/dt_f")).unwrap_or(false) {
                        for (i, t) in toks.iter().skip(1).take(3).enumerate() {
                            m.ramp.dv_dt_f[i] = parse_ratio(t);
                        }
                    }
                }
            }
            Section::None => {
                // [Model] sub-parameters appear outside any table section.
                if let Some(m) = cur_model.as_mut() {
                    match toks.first().map(|t| t.to_ascii_lowercase()).as_deref() {
                        Some("model_type") => {
                            m.model_type = toks.get(1).unwrap_or(&"").to_ascii_lowercase();
                        }
                        Some("c_comp") => {
                            for (i, t) in toks.iter().skip(1).take(3).enumerate() {
                                m.c_comp[i] = num(t);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    flush_component(&mut out, cur_component.take());
    flush_model(&mut out, cur_model.take());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal, clearly-synthetic 5V CMOS output buffer — hand-authored
    /// TEST data, not any vendor's part.
    const GOLDEN: &str = r#"
[IBIS Ver]   4.2
[File Name]  test_buffer.ibs
|
[Component]  TESTCHIP
[Manufacturer] BHDL Test Suite
[Pin]  signal_name  model_name
1      OUT1         CMOS_OUT
2      IN1          CMOS_IN
3      GND          GND
4      VCC          POWER
5      SEL          SELPIN
|
[Model Selector] SELPIN
CMOS_OUT   default output
CMOS_IN    alternate input
|
[Model]  CMOS_OUT
Model_type   Output
C_comp       3.5pF  3.0pF  4.0pF
[Voltage Range]  5.0V  4.5V  5.5V
[Pulldown]
| V        I(typ)     I(min)    I(max)
-5.0V     -0.080     -0.060    -0.100
 0.0V      0.000      0.000     0.000
 2.5V      0.040      0.030     0.050
 5.0V      0.060      0.045     0.075
 10.0V     0.062      NA        0.078
[Pullup]
| V(=Vcc-Vpin)  I(typ)   I(min)   I(max)
-5.0V      0.080      0.060     0.100
 0.0V      0.000      0.000     0.000
 2.5V     -0.040     -0.030    -0.050
 5.0V     -0.060     -0.045    -0.075
[GND Clamp]
-5.0V     -1.000     -0.800    -1.200
-0.7V     -0.001     -0.001    -0.001
 0.0V      0.000      0.000     0.000
[POWER Clamp]
-5.0V      1.000      0.800     1.200
-0.7V      0.001      0.001     0.001
 0.0V      0.000      0.000     0.000
[Ramp]
dV/dt_r    2.0/1.0n   1.5/1.2n  2.5/0.8n
dV/dt_f    2.0/1.1n   1.5/1.3n  2.5/0.9n
|
[Model]  CMOS_IN
Model_type   Input
[Voltage Range]  5.0V  4.5V  5.5V
[GND Clamp]
-5.0V     -1.000     -0.800    -1.200
 0.0V      0.000      0.000     0.000
|
[End]
"#;

    #[test]
    fn parses_structure() {
        let f = parse_str(GOLDEN).unwrap();
        assert_eq!(f.ibis_ver, "4.2");
        let c = f.component("TESTCHIP").unwrap();
        assert_eq!(c.pins.len(), 5);
        assert_eq!(c.pin_for("OUT1").unwrap().model_name, "CMOS_OUT");
        assert_eq!(c.pin_for("1").unwrap().signal_name, "OUT1");
        assert!(f.models.contains_key("CMOS_OUT"));
        assert_eq!(f.models["CMOS_OUT"].model_type, "output");
    }

    #[test]
    fn iv_tables_and_corners() {
        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        // Pulldown typ at 2.5V = 40mA exactly (table point).
        let i = m.iv_at(IvElement::Pulldown, Corner::Typ, 2.5).unwrap();
        assert!((i - 0.040).abs() < 1e-9);
        // Interpolated halfway 0..2.5 → 20mA.
        let i = m.iv_at(IvElement::Pulldown, Corner::Typ, 1.25).unwrap();
        assert!((i - 0.020).abs() < 1e-9);
        // NA in min column at 10V: last min point is 5.0V/0.045 → clamps.
        let i = m.iv_at(IvElement::Pulldown, Corner::Min, 10.0).unwrap();
        assert!((i - 0.045).abs() < 1e-9);
        // C_comp corners parsed with pF suffix.
        assert!((m.c_comp[0].unwrap() - 3.5e-12).abs() < 1e-15);
    }

    #[test]
    fn pullup_is_vcc_relative() {
        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        // At pin = Vcc (5.0V), table coordinate = 0 → I = 0.
        let i = m.iv_at(IvElement::Pullup, Corner::Typ, 5.0).unwrap();
        assert!(i.abs() < 1e-9);
        // At pin = 2.5V, table coord = 2.5 → −40mA (sourcing).
        let i = m.iv_at(IvElement::Pullup, Corner::Typ, 2.5).unwrap();
        assert!((i + 0.040).abs() < 1e-9);
        // Min corner uses Vcc(min)=4.5: pin 4.5 → coord 0 → 0.
        let i = m.iv_at(IvElement::Pullup, Corner::Min, 4.5).unwrap();
        assert!(i.abs() < 1e-9);
    }

    #[test]
    fn model_selector_resolves_to_first() {
        let f = parse_str(GOLDEN).unwrap();
        let m = f.resolve_model("SELPIN").unwrap();
        assert_eq!(m.name, "CMOS_OUT");
        // Specials resolve to nothing.
        assert!(f.resolve_model("POWER").is_none());
        assert!(f.resolve_model("GND").is_none());
    }

    #[test]
    fn composed_iv_high_state() {
        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        let pts = m.composed_iv(BufferState::High, Corner::Typ).unwrap();
        // At pin = 5V (=Vcc): pullup 0, clamps 0 → 0.
        let at = |v: f64| -> f64 {
            // reuse the solver-side interp semantics: clamp outside
            if v <= pts[0].0 { return pts[0].1; }
            if v >= pts[pts.len()-1].0 { return pts[pts.len()-1].1; }
            for w in pts.windows(2) {
                if v <= w[1].0 {
                    let t = (v - w[0].0) / (w[1].0 - w[0].0);
                    return w[0].1 + t * (w[1].1 - w[0].1);
                }
            }
            pts[pts.len()-1].1
        };
        assert!(at(5.0).abs() < 1e-9);
        // At pin = 2.5V: pullup −40mA sourcing.
        assert!((at(2.5) + 0.040).abs() < 1e-9);
        // HiZ state: clamps only → 0 everywhere in 0..Vcc.
        let hiz = m.composed_iv(BufferState::HiZ, Corner::Typ).unwrap();
        let mid = hiz.iter().find(|(v, _)| (*v - 0.0).abs() < 1e-9).unwrap();
        assert!(mid.1.abs() < 1e-9);
    }

    /// End-to-end DC: the golden buffer driving HIGH into a 125Ω load.
    /// Load-line intersection on the pullup segment (2.5V,−40mA)→(5V,0):
    /// i_buf(v) = 0.016·v − 0.08; i_load = v/125 ⇒ v = 3.3333V.
    #[test]
    fn buffer_drives_resistive_load() {
        use crate::circuit::{encode_iv_table, Circuit, META_IV_TABLE};
        use crate::glacier_dc_solver::GlacierDcSolver;
        use std::collections::HashMap;

        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        let pts = m.composed_iv(BufferState::High, Corner::Typ).unwrap();

        let mut c = Circuit::new();
        c.add_node("PIN".into(), None);
        c.add_node("GND".into(), None);
        let mut meta = HashMap::new();
        meta.insert(META_IV_TABLE.to_string(), encode_iv_table(&pts));
        c.add_branch_with_metadata(
            "buf".into(), "PIN", "GND", "IbisBuffer".into(), 0.0, None, meta,
        );
        c.add_branch("rload".into(), "PIN", "GND", "Resistor".into(), 125.0, None);

        let solver = GlacierDcSolver::new();
        let result = solver.solve(c.clone()).expect("solve");
        let pin_idx = c.nodes().find(|(_, n)| n.name == "PIN").unwrap().0;
        let v = result.node_voltages[&pin_idx];
        assert!(
            (v - 10.0 / 3.0).abs() < 1e-3,
            "expected 3.3333V at the load line, got {v}"
        );
    }

    #[test]
    fn ramp_parses() {
        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        let (dv, dt) = m.ramp.dv_dt_r[0].unwrap();
        assert!((dv - 2.0).abs() < 1e-12);
        assert!((dt - 1.0e-9).abs() < 1e-18);
    }

    /// The full fixture topology (rail source + buffer HIGH + 124Ω E-snapped
    /// load): the operating point must survive the system context. Load line
    /// on the pullup segment: 0.016·v − 0.08 + v/124 = 0 ⇒ v ≈ 3.3243V.
    #[test]
    fn fixture_topology_operating_point() {
        use crate::circuit::{encode_iv_table, Circuit, META_IV_TABLE};
        use crate::glacier_dc_solver::GlacierDcSolver;
        use std::collections::HashMap;

        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        let pts = m.composed_iv(BufferState::High, Corner::Typ).unwrap();

        let mut c = Circuit::new();
        c.add_node("VCC".into(), None);
        c.add_node("load_node".into(), None);
        c.add_node("GND".into(), None);
        c.add_branch("vcc_src".into(), "VCC", "GND", "VoltageSource".into(), 5.0, None);
        let mut meta = HashMap::new();
        meta.insert(META_IV_TABLE.to_string(), encode_iv_table(&pts));
        c.add_branch_with_metadata(
            "u1_OUT1_ibis".into(), "load_node", "GND", "IbisBuffer".into(), 0.0, None, meta,
        );
        c.add_branch("r_load".into(), "load_node", "GND", "Resistor".into(), 124.0, None);

        let result = GlacierDcSolver::new().solve(c.clone()).expect("solve");
        let idx = c.nodes().find(|(_, n)| n.name == "load_node").unwrap().0;
        let v = result.node_voltages[&idx];
        assert!((v - 3.3243).abs() < 5e-3, "expected load-line 3.3243V, got {v}");
    }
}
