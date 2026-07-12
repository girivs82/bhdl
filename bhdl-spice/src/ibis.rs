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

/// One `[Rising Waveform]` / `[Falling Waveform]` section: the pin
/// voltage vs time measured into a named resistive fixture. Multiple
/// sections per model (different fixtures) enable the two-waveform
/// Ku/Kd switching-coefficient solve.
#[derive(Debug, Clone, Default)]
pub struct Waveform {
    /// `R_fixture=` ohms.
    pub r_fixture: Option<f64>,
    /// `V_fixture=` volts (typ); `V_fixture_min/max=` per corner.
    pub v_fixture: [Option<f64>; 3],
    /// (time, voltage) points as written, per corner.
    pub typ: Vec<(f64, f64)>,
    pub min: Vec<(f64, f64)>,
    pub max: Vec<(f64, f64)>,
}

impl Waveform {
    pub fn points(&self, corner: Corner) -> &[(f64, f64)] {
        match corner {
            Corner::Typ => &self.typ,
            Corner::Min => &self.min,
            Corner::Max => &self.max,
        }
    }

    /// Fixture voltage for `corner`, falling back to typ (the file's own
    /// declaration order — V_fixture is mandatory, the corner variants
    /// optional).
    pub fn v_fix(&self, corner: Corner) -> Option<f64> {
        let idx = match corner {
            Corner::Typ => 0,
            Corner::Min => 1,
            Corner::Max => 2,
        };
        self.v_fixture[idx].or(self.v_fixture[0])
    }

    /// Piecewise-linear voltage at time `t`, clamped to the end values
    /// outside the table span. None if the corner column is empty.
    pub fn v_at(&self, corner: Corner, t: f64) -> Option<f64> {
        let pts = self.points(corner);
        if pts.is_empty() {
            return None;
        }
        if t <= pts[0].0 {
            return Some(pts[0].1);
        }
        if t >= pts[pts.len() - 1].0 {
            return Some(pts[pts.len() - 1].1);
        }
        for w in pts.windows(2) {
            if t <= w[1].0 {
                let f = (t - w[0].0) / (w[1].0 - w[0].0);
                return Some(w[0].1 + f * (w[1].1 - w[0].1));
            }
        }
        Some(pts[pts.len() - 1].1)
    }
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
    pub rising: Vec<Waveform>,
    pub falling: Vec<Waveform>,
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
        let (ku, kd) = match state {
            BufferState::High => (1.0, 0.0),
            BufferState::Low => (0.0, 1.0),
            BufferState::HiZ => (0.0, 0.0),
        };
        self.composed_iv_weighted(ku, kd, corner)
    }

    /// [`composed_iv`][Self::composed_iv] generalized to fractional drive
    /// weights: `i(v) = ku·I_pullup(v) + kd·I_pulldown(v) + clamps(v)`.
    /// This is the transient form — Ku(t)/Kd(t) from
    /// [`switching_coefficients`][Self::switching_coefficients] describe
    /// the buffer mid-transition. Weights of exactly 0 drop the element
    /// (so Hi-Z composes clamps only, as before).
    pub fn composed_iv_weighted(
        &self,
        ku: f64,
        kd: f64,
        corner: Corner,
    ) -> Option<Vec<(f64, f64)>> {
        let mut elements: Vec<(IvElement, f64)> = vec![
            (IvElement::GndClamp, 1.0),
            (IvElement::PowerClamp, 1.0),
        ];
        if ku != 0.0 {
            elements.push((IvElement::Pullup, ku));
        }
        if kd != 0.0 {
            elements.push((IvElement::Pulldown, kd));
        }

        // Union of breakpoints, in pin volts.
        let mut breakpoints: Vec<f64> = Vec::new();
        let mut any = false;
        for (el, _) in &elements {
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
                    .filter_map(|(el, w)| self.iv_at(*el, corner, v_pin).map(|i| w * i))
                    .sum();
                (v_pin, i)
            })
            .collect();
        Some(pts)
    }

    /// [`composed_iv_weighted`][Self::composed_iv_weighted] split by
    /// physical return rail, for per-rail current attribution:
    ///
    /// * `.0` — the GND-referenced group (`kd·pulldown + gnd_clamp`) in
    ///   PIN volts, for a branch pin→GND.
    /// * `.1` — the VCC-referenced group (`ku·pullup + power_clamp`) in
    ///   **branch volts `v = V_pin − V_rail`** (= −v_table, since those
    ///   tables are Vcc-relative), for a branch pin→VCC. Hanging it on
    ///   the actual rail node both books the sourced current against the
    ///   rail and lets the table track rail droop — the Vcc-relative
    ///   definition IS "relative to the rail", not to a constant.
    ///
    /// Each side is None when nothing in its group conducts. The two
    /// groups sum to the single-branch composite when the rail sits at
    /// the file's nominal Vcc.
    pub fn composed_iv_split(
        &self,
        ku: f64,
        kd: f64,
        corner: Corner,
    ) -> (Option<Vec<(f64, f64)>>, Option<Vec<(f64, f64)>>) {
        // BOTH groups are tabulated over the FULL composite breakpoint
        // span (all four elements, in pin volts). A group's own tables
        // may end mid-span — e.g. a GND clamp stops at (0V, 0A) — and a
        // table that ends where the OTHER group keeps operating would
        // leave the solver extrapolating through the clamp's knee slope,
        // injecting phantom current at real operating points. Evaluating
        // every element with its own clamp-flat semantics (`iv_at`) over
        // the shared span keeps "off beyond its table" honest, and solver
        // extrapolation only engages beyond the characterized range —
        // exactly as for the single-branch composite.
        let mut breakpoints: Vec<f64> = Vec::new();
        for (table, vcc_relative) in [
            (self.pulldown.as_ref(), false),
            (self.gnd_clamp.as_ref(), false),
            (self.pullup.as_ref(), true),
            (self.power_clamp.as_ref(), true),
        ] {
            let Some(table) = table else { continue };
            for (v, _) in table.points(corner) {
                let v_pin = if vcc_relative {
                    match self.vcc(corner) {
                        Some(vcc) => vcc - v,
                        None => continue,
                    }
                } else {
                    *v
                };
                breakpoints.push(v_pin);
            }
        }
        if breakpoints.is_empty() {
            return (None, None);
        }
        breakpoints.sort_by(|a, b| a.partial_cmp(b).unwrap());
        breakpoints.dedup_by(|a, b| (*a - *b).abs() < 1e-12);

        let gnd_active = self.gnd_clamp.is_some() || (kd != 0.0 && self.pulldown.is_some());
        let vcc_active = self.power_clamp.is_some() || (ku != 0.0 && self.pullup.is_some());

        let gnd = gnd_active.then(|| {
            breakpoints
                .iter()
                .map(|&v_pin| {
                    let mut i = self
                        .iv_at(IvElement::GndClamp, corner, v_pin)
                        .unwrap_or(0.0);
                    if kd != 0.0 {
                        i += kd
                            * self.iv_at(IvElement::Pulldown, corner, v_pin).unwrap_or(0.0);
                    }
                    (v_pin, i)
                })
                .collect::<Vec<_>>()
        });
        // VCC group in branch volts v = V_pin − Vcc (rail-referenced).
        let vcc = match (vcc_active, self.vcc(corner)) {
            (true, Some(vcc)) => Some(
                breakpoints
                    .iter()
                    .map(|&v_pin| {
                        let mut i = self
                            .iv_at(IvElement::PowerClamp, corner, v_pin)
                            .unwrap_or(0.0);
                        if ku != 0.0 {
                            i += ku
                                * self.iv_at(IvElement::Pullup, corner, v_pin).unwrap_or(0.0);
                        }
                        (v_pin - vcc, i)
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        };
        (gnd, vcc)
    }

    /// C_comp for `corner`, falling back to typ.
    pub fn c_comp_at(&self, corner: Corner) -> Option<f64> {
        let idx = match corner {
            Corner::Typ => 0,
            Corner::Min => 1,
            Corner::Max => 2,
        };
        self.c_comp[idx].or(self.c_comp[0])
    }

    /// Switching coefficients Ku(t)/Kd(t) for one edge, from the file's
    /// own transient data — best available method, never fabricated:
    ///
    /// 1. **Two waveforms** with distinct fixtures: at each time point the
    ///    pin KCL of each fixture gives one equation
    ///    `Ku·I_pu(V) + Kd·I_pd(V) + I_clamps(V) + C_comp·dV/dt +
    ///    (V − V_fix)/R_fix = 0`; two fixtures → a 2×2 solve for (Ku, Kd).
    /// 2. **One waveform**: the standard `Kd = 1 − Ku` reduction.
    /// 3. **No waveforms**: `[Ramp]` linearization — dV/dt covers the
    ///    20–80% swing, so a linear full transition lasts `dt/0.6`.
    ///
    /// Rows are `(t_seconds, ku, kd)` with t starting at the waveform
    /// table's own origin; coefficients are clamped to [0, 1] (table
    /// noise in flat regions otherwise produces small excursions).
    /// Returns None when the model has no drive elements or no transient
    /// data at all for the edge.
    pub fn switching_coefficients(
        &self,
        rising: bool,
        corner: Corner,
    ) -> Option<Vec<(f64, f64, f64)>> {
        if self.pullup.is_none() && self.pulldown.is_none() {
            return None;
        }
        let waves: Vec<&Waveform> = (if rising { &self.rising } else { &self.falling })
            .iter()
            .filter(|w| {
                !w.points(corner).is_empty()
                    && w.r_fixture.map(|r| r > 0.0).unwrap_or(false)
                    && w.v_fix(corner).is_some()
            })
            .collect();

        if waves.is_empty() {
            // [Ramp] fallback — linear coefficient sweep over dt/0.6.
            let idx = match corner {
                Corner::Typ => 0,
                Corner::Min => 1,
                Corner::Max => 2,
            };
            let slot = if rising { &self.ramp.dv_dt_r } else { &self.ramp.dv_dt_f };
            let (_dv, dt) = slot[idx].or(slot[0])?;
            if dt <= 0.0 {
                return None;
            }
            let t_full = dt / 0.6;
            return Some(if rising {
                vec![(0.0, 0.0, 1.0), (t_full, 1.0, 0.0)]
            } else {
                vec![(0.0, 1.0, 0.0), (t_full, 0.0, 1.0)]
            });
        }

        // Prefer a pair with distinct fixture VOLTAGES (best conditioning:
        // the same pin voltage sees genuinely different fixture currents),
        // else distinct resistances.
        let pair: Option<(&Waveform, &Waveform)> = waves
            .iter()
            .enumerate()
            .flat_map(|(i, a)| waves.iter().skip(i + 1).map(move |b| (*a, *b)))
            .find(|(a, b)| (a.v_fix(corner).unwrap() - b.v_fix(corner).unwrap()).abs() > 1e-3)
            .or_else(|| {
                waves
                    .iter()
                    .enumerate()
                    .flat_map(|(i, a)| waves.iter().skip(i + 1).map(move |b| (*a, *b)))
                    .find(|(a, b)| {
                        (a.r_fixture.unwrap() - b.r_fixture.unwrap()).abs()
                            > 1e-3 * a.r_fixture.unwrap()
                    })
            });

        let c_comp = self.c_comp_at(corner).unwrap_or(0.0);
        let clamps = |v: f64| -> f64 {
            self.iv_at(IvElement::GndClamp, corner, v).unwrap_or(0.0)
                + self.iv_at(IvElement::PowerClamp, corner, v).unwrap_or(0.0)
        };
        // Residual fixture-side current for waveform `w` at (t, V): every
        // term of the pin KCL except the Ku/Kd drive terms.
        let rest = |w: &Waveform, v: f64, dv_dt: f64| -> f64 {
            clamps(v) + c_comp * dv_dt + (v - w.v_fix(corner).unwrap()) / w.r_fixture.unwrap()
        };
        let dv_dt_of = |w: &Waveform, t: f64| -> f64 {
            let d = 1e-12;
            let a = w.v_at(corner, t - d).unwrap_or(0.0);
            let b = w.v_at(corner, t + d).unwrap_or(0.0);
            (b - a) / (2.0 * d)
        };
        let clamp01 = |x: f64| x.clamp(0.0, 1.0);

        // Merged time grid over the participating waveforms.
        let grid_of = |ws: &[&Waveform]| -> Vec<f64> {
            let mut g: Vec<f64> = ws
                .iter()
                .flat_map(|w| w.points(corner).iter().map(|(t, _)| *t))
                .collect();
            g.sort_by(|a, b| a.partial_cmp(b).unwrap());
            g.dedup_by(|a, b| (*a - *b).abs() < 1e-15);
            g
        };

        let out: Vec<(f64, f64, f64)> = match pair {
            Some((w1, w2)) => {
                let grid = grid_of(&[w1, w2]);
                let mut prev = if rising { (0.0, 1.0) } else { (1.0, 0.0) };
                grid.into_iter()
                    .map(|t| {
                        let v1 = w1.v_at(corner, t).unwrap();
                        let v2 = w2.v_at(corner, t).unwrap();
                        let a1 = self.iv_at(IvElement::Pullup, corner, v1).unwrap_or(0.0);
                        let b1 = self.iv_at(IvElement::Pulldown, corner, v1).unwrap_or(0.0);
                        let a2 = self.iv_at(IvElement::Pullup, corner, v2).unwrap_or(0.0);
                        let b2 = self.iv_at(IvElement::Pulldown, corner, v2).unwrap_or(0.0);
                        let r1 = -rest(w1, v1, dv_dt_of(w1, t));
                        let r2 = -rest(w2, v2, dv_dt_of(w2, t));
                        let det = a1 * b2 - a2 * b1;
                        let scale = (a1.abs() + a2.abs()).max(1e-15)
                            * (b1.abs() + b2.abs()).max(1e-15);
                        let (ku, kd) = if det.abs() > 1e-6 * scale {
                            ((r1 * b2 - r2 * b1) / det, (a1 * r2 - a2 * r1) / det)
                        } else {
                            // Ill-conditioned (waveforms at the same
                            // operating point) — hold the previous values.
                            prev
                        };
                        let ku = clamp01(ku);
                        let kd = clamp01(kd);
                        prev = (ku, kd);
                        (t, ku, kd)
                    })
                    .collect()
            }
            None => {
                // Single usable fixture: Kd = 1 − Ku.
                let w = waves[0];
                let grid = grid_of(&[w]);
                let mut prev = if rising { (0.0, 1.0) } else { (1.0, 0.0) };
                grid.into_iter()
                    .map(|t| {
                        let v = w.v_at(corner, t).unwrap();
                        let a = self.iv_at(IvElement::Pullup, corner, v).unwrap_or(0.0);
                        let b = self.iv_at(IvElement::Pulldown, corner, v).unwrap_or(0.0);
                        let r = -rest(w, v, dv_dt_of(w, t));
                        let denom = a - b;
                        let ku = if denom.abs() > 1e-12 {
                            clamp01((r - b) / denom)
                        } else {
                            prev.0
                        };
                        let kd = clamp01(1.0 - ku);
                        prev = (ku, kd);
                        (t, ku, kd)
                    })
                    .collect()
            }
        };
        Some(out)
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

/// `[Package]` — lumped package parasitics per corner (typ/min/max).
/// The lead's series R/L and shunt C; per-pin overrides may refine
/// these in the [Pin] table's R_pin/L_pin/C_pin columns.
#[derive(Debug, Clone, Default)]
pub struct PackageRlc {
    pub r_pkg: [Option<f64>; 3],
    pub l_pkg: [Option<f64>; 3],
    pub c_pkg: [Option<f64>; 3],
}

impl PackageRlc {
    /// Lead inductance for `corner`, falling back to typ.
    pub fn l(&self, corner: Corner) -> Option<f64> {
        let idx = match corner {
            Corner::Typ => 0,
            Corner::Min => 1,
            Corner::Max => 2,
        };
        self.l_pkg[idx].or(self.l_pkg[0])
    }
}

/// One `[Pin]` row of a `[Component]`.
#[derive(Debug, Clone)]
pub struct Pin {
    pub pin: String,         // package pin number/name ("29", "A4")
    pub signal_name: String, // vendor's signal name ("PD2", "DQ0")
    pub model_name: String,  // model or model-selector name; "NC"/"GND"/"POWER" specials
    /// Per-pin parasitic overrides (R_pin/L_pin/C_pin columns) — most
    /// GENIBIS files declare the columns but leave them empty, in which
    /// case the [Package] lump applies.
    pub r_pin: Option<f64>,
    pub l_pin: Option<f64>,
    pub c_pin: Option<f64>,
}

/// One `[Component]` section.
#[derive(Debug, Clone, Default)]
pub struct Component {
    pub name: String,
    pub manufacturer: String,
    pub pins: Vec<Pin>,
    pub package: PackageRlc,
}

impl Component {
    /// The component's primary power-rail pin: the POWER row whose
    /// signal name is exactly "VCC" (the IBIS-conventional primary
    /// rail), else a SOLE POWER row. None when several POWER rails
    /// exist and none is named VCC — without a [Pin Mapping] section
    /// the file doesn't say which rail feeds which buffer, and we
    /// don't guess.
    pub fn power_pin(&self) -> Option<&Pin> {
        let powers: Vec<&Pin> = self
            .pins
            .iter()
            .filter(|p| p.model_name.eq_ignore_ascii_case("POWER"))
            .collect();
        powers
            .iter()
            .find(|p| p.signal_name.eq_ignore_ascii_case("VCC"))
            .copied()
            .or(if powers.len() == 1 { Some(powers[0]) } else { None })
    }

    /// Effective lead inductance for a pin at `corner`: the per-pin
    /// override when the file carries one, else the [Package] lump.
    pub fn pin_inductance(&self, pin: &Pin, corner: Corner) -> Option<f64> {
        pin.l_pin.or_else(|| self.package.l(corner))
    }

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
        Package,
        /// Data rows of the most recent [Rising/Falling Waveform]
        /// section (true = rising). The Waveform struct itself was
        /// already pushed onto the model when the header was seen.
        Wave(bool),
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
            // Real generators vary keyword spelling: GENIBIS emits
            // `[GND_clamp]` / `[POWER_clamp]` (underscores) where the spec
            // writes `[GND Clamp]`. Normalize separators before matching.
            let keyword = rest[..close].trim().to_ascii_lowercase().replace('_', " ");
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
                "package" => section = Section::Package,
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
                "rising waveform" => {
                    if let Some(m) = cur_model.as_mut() {
                        m.rising.push(Waveform::default());
                        section = Section::Wave(true);
                    }
                }
                "falling waveform" => {
                    if let Some(m) = cur_model.as_mut() {
                        m.falling.push(Waveform::default());
                        section = Section::Wave(false);
                    }
                }
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
                            r_pin: toks.get(3).and_then(|t| num(t)),
                            l_pin: toks.get(4).and_then(|t| num(t)),
                            c_pin: toks.get(5).and_then(|t| num(t)),
                        });
                    }
                }
            }
            Section::Package => {
                if let Some(c) = cur_component.as_mut() {
                    let slot = match toks.first().map(|t| t.to_ascii_lowercase()).as_deref() {
                        Some("r_pkg") => Some(&mut c.package.r_pkg),
                        Some("l_pkg") => Some(&mut c.package.l_pkg),
                        Some("c_pkg") => Some(&mut c.package.c_pkg),
                        _ => None,
                    };
                    if let Some(slot) = slot {
                        for (i, t) in toks.iter().skip(1).take(3).enumerate() {
                            slot[i] = num(t);
                        }
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
            Section::Wave(rising) => {
                let Some(m) = cur_model.as_mut() else { continue };
                let Some(w) = (if *rising { m.rising.last_mut() } else { m.falling.last_mut() })
                else { continue };
                // Fixture sub-parameters: `R_fixture= 50.0000` (GENIBIS
                // spacing) or `R_fixture=50`. Split on '=' first.
                if let Some((key, val)) = line.split_once('=') {
                    let key = key.trim().to_ascii_lowercase();
                    let val = val.trim();
                    match key.as_str() {
                        "r_fixture" => w.r_fixture = num(val),
                        "v_fixture" => w.v_fixture[0] = num(val),
                        "v_fixture_min" => w.v_fixture[1] = num(val),
                        "v_fixture_max" => w.v_fixture[2] = num(val),
                        // L_fixture / C_fixture etc: unmodeled — the
                        // vendor sweep shows GENIBIS emits none.
                        _ => {}
                    }
                    continue;
                }
                // Data row: time then typ/min/max voltages.
                if toks.len() >= 2 {
                    if let Some(t) = num(toks[0]) {
                        if let Some(v) = toks.get(1).and_then(|x| num(x)) {
                            w.typ.push((t, v));
                        }
                        if let Some(v) = toks.get(2).and_then(|x| num(x)) {
                            w.min.push((t, v));
                        }
                        if let Some(v) = toks.get(3).and_then(|x| num(x)) {
                            w.max.push((t, v));
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
    fn waveform_sections_parse() {
        let text = r#"
[IBIS Ver] 4.2
[File Name] w.ibs
[Component] T
[Pin] signal_name model_name
1 OUT M
[Model] M
Model_type Output
[Voltage Range] 5.0 NA NA
[Pulldown]
0.0 0.0 NA NA
5.0 50mA NA NA
[Pullup]
0.0 0.0 NA NA
5.0 -50mA NA NA
[Rising Waveform]
R_fixture= 50.0000
V_fixture= 0.0
V_fixture_min= 0.0
V_fixture_max= 0.0
|time V(typ) V(min) V(max)
0.0S 0.1V NA NA
1.0nS 0.5V NA NA
2.0nS 2.0V NA NA
[Rising Waveform]
R_fixture= 2.0000k
V_fixture= 5.0
0.0S 4.0V NA NA
2.0nS 4.9V NA NA
[Falling Waveform]
R_fixture=50
V_fixture=5.0
0.0S 4.9V NA NA
2.0nS 2.5V NA NA
[Ramp]
dV/dt_r 3.0/1.2n NA NA
[End]
"#;
        let f = parse_str(text).unwrap();
        let m = &f.models["M"];
        assert_eq!(m.rising.len(), 2);
        assert_eq!(m.falling.len(), 1);
        let w0 = &m.rising[0];
        assert_eq!(w0.r_fixture, Some(50.0));
        assert_eq!(w0.v_fixture[0], Some(0.0));
        assert_eq!(w0.typ.len(), 3);
        assert!((w0.typ[1].0 - 1e-9).abs() < 1e-15);
        // interpolation midway between rows
        assert!((w0.v_at(Corner::Typ, 1.5e-9).unwrap() - 1.25).abs() < 1e-9);
        // '=' with no space also parses
        assert_eq!(m.falling[0].r_fixture, Some(50.0));
        assert_eq!(m.rising[1].v_fixture[0], Some(5.0));
    }

    /// [Ramp]-only model: coefficients are the linear sweep over the
    /// full-swing time dt/0.6, endpoints exactly (0,1) → (1,0).
    #[test]
    fn switching_coefficients_ramp_fallback() {
        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        let ks = m.switching_coefficients(true, Corner::Typ).unwrap();
        assert_eq!(ks.len(), 2);
        assert_eq!(ks[0], (0.0, 0.0, 1.0));
        let (t, ku, kd) = ks[1];
        assert!((ku, kd) == (1.0, 0.0));
        // GOLDEN [Ramp] dV/dt_r = 2.0/1.0n → t_full = 1.0n/0.6.
        assert!((t - 1.0e-9 / 0.6).abs() < 1e-15, "t_full = {t}");
    }

    /// REAL Atmel data (existence-gated): two-waveform Ku/Kd extraction
    /// on the 16U2 gpio model — starts at (0,1), ends at (1,≈0), and the
    /// pulldown lets go before the pullup takes over (break-before-make,
    /// straight from the vendor tables).
    #[test]
    fn real_16u2_gpio_switching_coefficients() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/ibis/megaavr/m16u2m32.ibs");
        if !path.exists() {
            eprintln!("real_16u2_gpio_switching_coefficients: vendor file absent — skipped");
            return;
        }
        let f = parse_file(&path).unwrap();
        let m = &f.models["gpio"];
        assert_eq!(m.rising.len(), 4);
        let ks = m.switching_coefficients(true, Corner::Typ).unwrap();
        let (t0, ku0, kd0) = ks[0];
        let (_, kun, kdn) = *ks.last().unwrap();
        assert_eq!(t0, 0.0);
        assert!(ku0 < 0.05 && kd0 > 0.95, "start ({ku0},{kd0})");
        assert!(kun > 0.95 && kdn < 0.05, "end ({kun},{kdn})");
        let t_kd_off = ks.iter().find(|(_, _, kd)| *kd < 0.5).unwrap().0;
        let t_ku_on = ks.iter().find(|(_, ku, _)| *ku > 0.5).unwrap().0;
        assert!(
            t_kd_off < t_ku_on,
            "expected break-before-make: kd off at {t_kd_off}, ku on at {t_ku_on}"
        );
    }

    #[test]
    /// The rail-split groups must reconstruct the single-branch
    /// composite when the rail sits at nominal Vcc:
    /// full(v) = gnd_group(v) + vcc_group(v − Vcc).
    #[test]
    fn split_groups_sum_to_composite() {
        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        let vcc = m.vcc(Corner::Typ).unwrap();
        for (ku, kd) in [(1.0, 0.0), (0.0, 1.0), (0.3, 0.6)] {
            let full = m.composed_iv_weighted(ku, kd, Corner::Typ).unwrap();
            let (gnd, vc) = m.composed_iv_split(ku, kd, Corner::Typ);
            let interp = |pts: &Option<Vec<(f64, f64)>>, v: f64| -> f64 {
                let Some(pts) = pts else { return 0.0 };
                if v <= pts[0].0 { return pts[0].1; }
                if v >= pts[pts.len() - 1].0 { return pts[pts.len() - 1].1; }
                for w in pts.windows(2) {
                    if v <= w[1].0 {
                        return w[0].1
                            + (v - w[0].0) / (w[1].0 - w[0].0) * (w[1].1 - w[0].1);
                    }
                }
                pts[pts.len() - 1].1
            };
            for (v, i_full) in &full {
                let sum = interp(&gnd, *v) + interp(&vc, *v - vcc);
                assert!(
                    (sum - i_full).abs() < 1e-9,
                    "ku={ku} kd={kd} v={v}: split sum {sum} vs full {i_full}"
                );
            }
        }
        // power_pin picks the VCC row of the golden component.
        let c = f.component("TESTCHIP").unwrap();
        assert_eq!(c.power_pin().unwrap().signal_name, "VCC");
    }

    /// Rail-split DC: the GOLDEN buffer HIGH into 125Ω, stamped as TWO
    /// branches (gnd group pin→GND, vcc group pin→VCC). The operating
    /// point must equal the single-branch solve (3.3333V load line), and
    /// ALL the sourced current must be booked on the VCC-referenced
    /// branch — that is the per-rail attribution this split exists for.
    #[test]
    fn split_stamp_attributes_current_to_rail() {
        use crate::circuit::{encode_iv_table, Circuit, META_IV_TABLE};
        use crate::glacier_dc_solver::GlacierDcSolver;
        use std::collections::HashMap;

        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        let (gnd_pts, vcc_pts) = m.composed_iv_split(1.0, 0.0, Corner::Typ);
        let vcc_pts = vcc_pts.expect("pullup group");

        let mut c = Circuit::new();
        c.add_node("VCC".into(), None);
        c.add_node("PIN".into(), None);
        c.add_node("GND".into(), None);
        c.add_branch("vs".into(), "VCC", "GND", "VoltageSource".into(), 5.0, None);
        c.add_branch("load".into(), "PIN", "GND", "Resistor".into(), 125.0, None);
        let mut meta = HashMap::new();
        meta.insert(META_IV_TABLE.to_string(), encode_iv_table(&vcc_pts));
        c.add_branch_with_metadata(
            "buf_vcc".into(), "PIN", "VCC", "IbisBuffer".into(), 0.0, None, meta,
        );
        if let Some(pts) = &gnd_pts {
            let mut meta = HashMap::new();
            meta.insert(META_IV_TABLE.to_string(), encode_iv_table(pts));
            c.add_branch_with_metadata(
                "buf_gnd".into(), "PIN", "GND", "IbisBuffer".into(), 0.0, None, meta,
            );
        }

        let r = GlacierDcSolver::new().solve(c.clone()).expect("solve");
        let node = |n: &str| c.nodes().find(|(_, x)| x.name == n).unwrap().0;
        let vpin = r.node_voltages[&node("PIN")];
        assert!((vpin - 10.0 / 3.0).abs() < 1e-3, "pin {vpin} vs load-line 3.3333V");

        // The rail source must carry the load current: i(vs) ≈ v/125.
        let vs_edge = c.branches().find(|(_, b)| b.name == "vs").unwrap().0;
        let i_rail = r.branch_currents.get(&vs_edge).copied().unwrap_or(0.0).abs();
        let i_load = vpin / 125.0;
        assert!(
            (i_rail - i_load).abs() < 1e-4,
            "rail current {i_rail} A should equal load current {i_load} A"
        );
        eprintln!(
            "split stamp: pin {vpin:.4}V, rail supplies {:.2}mA (= load {:.2}mA)",
            i_rail * 1e3, i_load * 1e3
        );
    }

    /// REAL Atmel data (existence-gated): the PDIP-28 [Package] lump.
    #[test]
    fn real_package_parasitics() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/ibis/megaavr/m1682p28.ibs");
        if !path.exists() {
            eprintln!("real_package_parasitics: vendor file absent — skipped");
            return;
        }
        let f = parse_file(&path).unwrap();
        let c = f.component("atmega168_20_pdip28").unwrap();
        assert!((c.package.l_pkg[0].unwrap() - 7.11e-9).abs() < 1e-12);
        assert!((c.package.l_pkg[1].unwrap() - 3.72e-9).abs() < 1e-12);
        assert!((c.package.r_pkg[0].unwrap() - 42e-3).abs() < 1e-6);
        assert!((c.package.c_pkg[2].unwrap() - 2.2e-12).abs() < 1e-15);
        // This file declares the per-pin columns but leaves them empty:
        // the effective inductance falls back to the lump.
        let pin = c.pin_for("PD5").unwrap();
        assert!(pin.l_pin.is_none());
        assert!((c.pin_inductance(pin, Corner::Typ).unwrap() - 7.11e-9).abs() < 1e-12);
    }

    #[test]
    fn ramp_parses() {
        let f = parse_str(GOLDEN).unwrap();
        let m = &f.models["CMOS_OUT"];
        let (dv, dt) = m.ramp.dv_dt_r[0].unwrap();
        assert!((dv - 2.0).abs() < 1e-12);
        assert!((dt - 1.0e-9).abs() < 1e-18);
    }

    /// Real vendor files (Atmel AT32UC3A3/A4, GENIBIS output) — parse all
    /// three package variants and solve a REAL GPIO buffer against a load.
    /// Gated on the files' presence: the Atmel license permits use but not
    /// redistribution, so vendor/ibis/ is gitignored and this test skips
    /// cleanly where the files are absent.
    #[test]
    fn real_atmel_uc3a3_files() {
        use crate::circuit::{encode_iv_table, Circuit, META_IV_TABLE};
        use crate::glacier_dc_solver::GlacierDcSolver;
        use std::collections::HashMap;

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../vendor/ibis");
        if !dir.join("u3a33t44.ibs").exists() {
            eprintln!("real_atmel_uc3a3_files: vendor files absent — skipped");
            return;
        }
        // All three package variants must parse with populated tables.
        for f in ["u3a33t44.ibs", "u3a33c44.ibs", "u3a43c00.ibs"] {
            let ib = parse_file(&dir.join(f)).expect(f);
            assert!(!ib.components.is_empty(), "{f}: no components");
            assert!(ib.models.len() >= 16, "{f}: models missing");
            let clamps: usize = ib.models.values()
                .map(|m| m.gnd_clamp.as_ref().map(|t| t.typ.len()).unwrap_or(0))
                .sum();
            assert!(clamps > 0, "{f}: clamp tables empty (keyword normalization?)");
        }

        // Solve the TQFP144 GPIO buffer (ct33x3b01up, i/o) HIGH into 150Ω.
        let ib = parse_file(&dir.join("u3a33t44.ibs")).unwrap();
        let m = &ib.models["ct33x3b01up"];
        let pts = m.composed_iv(BufferState::High, Corner::Typ).unwrap();
        let interp = |v: f64| -> f64 {
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
        // The table's own root of interp(v) + v/150 = 0 in (0, Vcc), by
        // bisection — the expected operating point from the vendor's data.
        let (mut lo, mut hi) = (0.0_f64, 3.3_f64);
        for _ in 0..60 {
            let mid = (lo + hi) / 2.0;
            if interp(mid) + mid / 150.0 > 0.0 { hi = mid } else { lo = mid }
        }
        let expected = (lo + hi) / 2.0;
        assert!(expected > 2.0 && expected < 3.3, "implausible root {expected}");

        let mut c = Circuit::new();
        c.add_node("PIN".into(), None);
        c.add_node("GND".into(), None);
        let mut meta = HashMap::new();
        meta.insert(META_IV_TABLE.to_string(), encode_iv_table(&pts));
        c.add_branch_with_metadata(
            "buf".into(), "PIN", "GND", "IbisBuffer".into(), 0.0, None, meta,
        );
        c.add_branch("rload".into(), "PIN", "GND", "Resistor".into(), 150.0, None);
        let result = GlacierDcSolver::new().solve(c.clone()).expect("solve");
        let idx = c.nodes().find(|(_, n)| n.name == "PIN").unwrap().0;
        let v = result.node_voltages[&idx];
        assert!(
            (v - expected).abs() < 2e-3,
            "solver {v} vs table root {expected}"
        );
        eprintln!("real UC3A3 GPIO HIGH into 150Ω: {v:.4}V (table root {expected:.4}V)");
    }

    /// The Uno's TX-LED path against Atmel's REAL 16U2 gpio buffer:
    /// 5V → LED (exponential) → 1kΩ → pin driven LOW (megaAVR pulldown
    /// table). Mixed nonlinearities in one solve. Existence-gated like the
    /// UC3 test. Sanity band: the pin clamps low (< 0.5V) and the LED
    /// conducts 2–4mA — the current the schematic used to ASSUME is now
    /// measured through vendor silicon data.
    #[test]
    fn real_16u2_led_path() {
        use crate::circuit::{encode_iv_table, Circuit, META_IV_TABLE};
        use crate::glacier_dc_solver::GlacierDcSolver;
        use std::collections::HashMap;

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/ibis/megaavr/m16u2m32.ibs");
        if !path.exists() {
            eprintln!("real_16u2_led_path: vendor file absent — skipped");
            return;
        }
        let ib = parse_file(&path).unwrap();
        let m = &ib.models["gpio"];
        let pts = m.composed_iv(BufferState::Low, Corner::Typ).unwrap();

        let mut c = Circuit::new();
        for n in ["VCC", "led_k", "pin", "GND"] {
            c.add_node(n.into(), None);
        }
        c.add_branch("vcc".into(), "VCC", "GND", "VoltageSource".into(), 5.0, None);
        c.add_branch("led".into(), "VCC", "led_k", "LED".into(), 0.0, None);
        c.add_branch("r".into(), "led_k", "pin", "Resistor".into(), 1000.0, None);
        let mut meta = HashMap::new();
        meta.insert(META_IV_TABLE.to_string(), encode_iv_table(&pts));
        c.add_branch_with_metadata(
            "pd4".into(), "pin", "GND", "IbisBuffer".into(), 0.0, None, meta,
        );

        let result = GlacierDcSolver::new().solve(c.clone()).expect("solve");
        let vpin = result.node_voltages
            [&c.nodes().find(|(_, n)| n.name == "pin").unwrap().0];
        let vled_k = result.node_voltages
            [&c.nodes().find(|(_, n)| n.name == "led_k").unwrap().0];
        let i_led = (vled_k - vpin) / 1000.0;
        eprintln!("16U2 PD4 LOW sinking TX LED: pin {vpin:.3}V, I_led {:.3}mA", i_led * 1e3);
        assert!(vpin < 0.5, "pin should clamp low, got {vpin}");
        assert!(i_led > 2e-3 && i_led < 4e-3, "LED current {i_led} out of band");
    }

    /// The 16U2's REAL USB D+ transceiver buffer (avrusb16k dm/dp file,
    /// model gpiopu3b01fc, 3.3V pad domain) driving the full-speed idle
    /// J-state: D+ HIGH into the host's 15kΩ pulldown. Expected operating
    /// point = the vendor table's own root (self-consistent, no hand-typed
    /// numbers). Existence-gated.
    #[test]
    fn real_16u2_usb_dp_idle() {
        use crate::circuit::{encode_iv_table, Circuit, META_IV_TABLE};
        use crate::glacier_dc_solver::GlacierDcSolver;
        use std::collections::HashMap;

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/ibis/megaavr/m04_tqfp32_dm_dp_avrusb16k_3v.ibs");
        if !path.exists() {
            eprintln!("real_16u2_usb_dp_idle: vendor file absent — skipped");
            return;
        }
        let ib = parse_file(&path).unwrap();
        let comp = ib.component("megausblc").expect("component");
        let dp = comp.pin_for("dp").expect("dp pin");
        let m = ib.resolve_model(&dp.model_name).expect("dp model");
        assert!(m.pullup.is_some() && m.gnd_clamp.is_some(), "full buffer expected");
        let pts = m.composed_iv(BufferState::High, Corner::Typ).unwrap();

        let interp = |v: f64| -> f64 {
            if v <= pts[0].0 { return pts[0].1; }
            if v >= pts[pts.len()-1].0 { return pts[pts.len()-1].1; }
            for w in pts.windows(2) {
                if v <= w[1].0 {
                    return w[0].1 + (v - w[0].0) / (w[1].0 - w[0].0) * (w[1].1 - w[0].1);
                }
            }
            pts[pts.len()-1].1
        };
        let (mut lo, mut hi) = (0.0_f64, 3.6_f64);
        for _ in 0..60 {
            let mid = (lo + hi) / 2.0;
            if interp(mid) + mid / 15000.0 > 0.0 { hi = mid } else { lo = mid }
        }
        let expected = (lo + hi) / 2.0;
        // USB FS spec wants V_OH ≥ 2.8V into 15kΩ — the vendor data should land there.
        assert!(expected > 2.8 && expected < 3.6, "implausible J-state {expected}");

        let mut c = Circuit::new();
        c.add_node("DP".into(), None);
        c.add_node("GND".into(), None);
        let mut meta = HashMap::new();
        meta.insert(META_IV_TABLE.to_string(), encode_iv_table(&pts));
        c.add_branch_with_metadata(
            "dp_buf".into(), "DP", "GND", "IbisBuffer".into(), 0.0, None, meta,
        );
        c.add_branch("host_pd".into(), "DP", "GND", "Resistor".into(), 15000.0, None);
        let result = GlacierDcSolver::new().solve(c.clone()).expect("solve");
        let v = result.node_voltages[&c.nodes().find(|(_, n)| n.name == "DP").unwrap().0];
        assert!((v - expected).abs() < 2e-3, "solver {v} vs table root {expected}");
        eprintln!("16U2 USB D+ idle J-state into 15kΩ: {v:.4}V (USB spec V_OH ≥ 2.8V)");
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
