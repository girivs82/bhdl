//! THE stage acceptance predicate (docs/spec/Requirements_And_Resolution.md
//! §3: "acceptance and resolution are the same predicate").
//!
//! One function, two callers:
//!
//! - the requirement RESOLVER asks "which blocks meet this requirement?"
//!   — promises read from the block's entity text before anything exists;
//! - ERC032 asks "does the committed stage still meet it?" — promises
//!   read from the synthesized instance's attributes on the flattened
//!   circuit, every build.
//!
//! Both build a [`StageRequirement`] and a [`StagePromises`] and call
//! [`check`]. A gate is `ok`, `failed`, or `unchecked` (the promise the
//! requirement needs is UNDECLARED — stated, never a pass). The derating
//! policy and every comparison live here and nowhere else.

/// The power-tree derating policy: a regulator runs at ≤ 80 % of
/// nameplate. `i_max` is the load; the rating a block must promise is
/// `i_max / CURRENT_DERATE`.
pub const CURRENT_DERATE: f64 = 0.8;

/// What is asked of the stage (SI units: A, V, °C; efficiency as a
/// fraction). `None` = not required → that gate is not evaluated.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StageRequirement {
    pub i_max_a: Option<f64>,
    pub vin_v: Option<f64>,
    pub vin_min_v: Option<f64>,
    pub vin_max_v: Option<f64>,
    pub noise_v: Option<f64>,
    pub efficiency_min: Option<f64>,
    pub phases: Option<f64>,
    pub temp_min_c: Option<f64>,
    pub temp_max_c: Option<f64>,
    pub qual: Option<String>,
    // PreregStage protection functions (distinct words, distinct behaviours)
    pub ov_clamp_v: Option<f64>,
    pub ov_trip_v: Option<f64>,
    pub uv_trip_v: Option<f64>,
    pub reverse_polarity: Option<bool>,
    /// Required ASIL capability of the part (from the safety analysis or
    /// a project-wide requirement): QM < A < B < C < D.
    pub asil: Option<Asil>,
    /// Dissipation the stage will run at (W) — supplied by the caller
    /// when it can compute it (resolver: from the operating point;
    /// ERC032: from the sign-off's p_diss) so an AMBIENT temperature
    /// requirement can be met THERMALLY by a junction-rated part:
    /// T_J = T_A,max + P·θ_JA ≤ T_J,max.
    pub p_diss_w: Option<f64>,
}

/// ASIL ordering for capability gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Asil {
    Qm,
    A,
    B,
    C,
    D,
}

impl Asil {
    pub fn parse(s: &str) -> Option<Asil> {
        match s.trim().trim_matches('"').to_ascii_uppercase().trim_start_matches("ASIL_").trim_start_matches("ASIL") {
            "QM" => Some(Asil::Qm),
            "A" => Some(Asil::A),
            "B" => Some(Asil::B),
            "C" => Some(Asil::C),
            "D" => Some(Asil::D),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Asil::Qm => "QM",
            Asil::A => "A",
            Asil::B => "B",
            Asil::C => "C",
            Asil::D => "D",
        }
    }
}

/// What the stage declares (SI units; efficiency as a fraction). `None`
/// = undeclared → UNCHECKED against a requirement that needs it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StagePromises {
    pub output_current_a: Option<f64>,
    pub vin_min_v: Option<f64>,
    pub vin_max_v: Option<f64>,
    pub output_noise_v: Option<f64>,
    pub efficiency: Option<f64>,
    pub phases: Option<f64>,
    pub temp_min_c: Option<f64>,
    pub temp_max_c: Option<f64>,
    pub qualification: Option<String>,
    pub ov_clamp_v: Option<f64>,
    pub ov_trip_v: Option<f64>,
    pub uv_trip_v: Option<f64>,
    pub reverse_polarity: Option<bool>,
    /// Vendor functional-safety capability claim (SEooC / FS-compliant
    /// documentation) — `attribute asil_capable = "B";`.
    pub asil_capable: Option<Asil>,
    /// Junction-to-ambient thermal resistance (°C/W) and the junction
    /// rating — the thermal path to an ambient requirement.
    pub theta_ja_c_per_w: Option<f64>,
    pub tj_max_c: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateVerdict {
    Ok,
    Failed,
    /// The needed promise is undeclared — a rejection for automatic
    /// binding, an Info finding for ERC032, never a pass.
    Unchecked,
}

#[derive(Debug, Clone)]
pub struct Gate {
    pub name: &'static str,
    pub detail: String,
    pub verdict: GateVerdict,
    /// What to declare to make the gate checkable (for UNCHECKED).
    pub declare: &'static str,
}

impl Gate {
    pub fn ok(&self) -> bool {
        self.verdict == GateVerdict::Ok
    }
    pub fn unchecked(&self) -> bool {
        self.verdict == GateVerdict::Unchecked
    }
}

/// The predicate. Gates are emitted only for what the requirement asks.
pub fn check(req: &StageRequirement, p: &StagePromises) -> Vec<Gate> {
    let mut g = Vec::new();
    let push = |g: &mut Vec<Gate>, name: &'static str, detail: String, ok: bool| {
        g.push(Gate { name, detail, verdict: if ok { GateVerdict::Ok } else { GateVerdict::Failed }, declare: "" });
    };
    let unchecked = |g: &mut Vec<Gate>, name: &'static str, detail: String, declare: &'static str| {
        g.push(Gate { name, detail, verdict: GateVerdict::Unchecked, declare });
    };

    if let Some(i_max) = req.i_max_a {
        let need = i_max / CURRENT_DERATE;
        match p.output_current_a {
            Some(r) => push(
                &mut g,
                "i_max",
                format!("output_current {r:.3}A ≥ required rating {need:.3}A (i_max {i_max:.3}A / {CURRENT_DERATE} derate)"),
                r + 1e-9 >= need,
            ),
            None => unchecked(&mut g, "i_max", format!("declares no output_current — required rating {need:.3}A UNCHECKED, not a pass"), "attribute output_current = <datasheet rating>;"),
        }
    }
    let vin_lo = req.vin_min_v.or(req.vin_v);
    let vin_hi = req.vin_max_v.or(req.vin_v);
    if let Some(lo) = vin_lo {
        match p.vin_min_v {
            Some(b) => push(&mut g, "vin_min", format!("vin_min {b:.2}V ≤ requirement {lo:.2}V"), b <= lo + 1e-9),
            None => unchecked(&mut g, "vin_min", "declares no vin_min — UNCHECKED, not a pass".into(), "attribute vin_min = <datasheet>;"),
        }
    }
    if let Some(hi) = vin_hi {
        match p.vin_max_v {
            Some(b) => push(&mut g, "vin_max", format!("vin_max {b:.2}V ≥ requirement {hi:.2}V"), b + 1e-9 >= hi),
            None => unchecked(&mut g, "vin_max", "declares no vin_max — UNCHECKED, not a pass".into(), "attribute vin_max = <datasheet>;"),
        }
    }
    if let Some(n) = req.noise_v {
        match p.output_noise_v {
            Some(b) => push(&mut g, "noise", format!("output noise {:.1}µVrms ≤ requirement {:.1}µVrms", b * 1e6, n * 1e6), b <= n + 1e-15),
            None => unchecked(
                &mut g,
                "noise",
                format!("declares no output_noise — output noise acceptance (≤ {:.0}µVrms) UNCHECKED, not a pass", n * 1e6),
                "attribute output_noise = <datasheet µVrms>; (switching parts state ripple, not noise — leave undeclared)",
            ),
        }
    }
    if let Some(e) = req.efficiency_min {
        match p.efficiency {
            Some(b) => push(&mut g, "efficiency", format!("efficiency {:.1}% ≥ requirement {:.1}%", b * 100.0, e * 100.0), b + 1e-9 >= e),
            None => unchecked(&mut g, "efficiency", format!("declares no efficiency — efficiency acceptance (≥ {:.1}%) UNCHECKED, not a pass", e * 100.0), "attribute efficiency = <datasheet mid-load figure>;"),
        }
    }
    if let Some(ph) = req.phases {
        match p.phases {
            Some(b) => push(&mut g, "phases", format!("supports {b:.0} phase(s) ≥ required {ph:.0}"), b + 1e-9 >= ph),
            None => unchecked(&mut g, "phases", "declares no phases — UNCHECKED, not a pass".into(), "attribute phases = <n>;"),
        }
    }
    if let Some(lo) = req.temp_min_c {
        match p.temp_min_c {
            Some(b) => push(&mut g, "temp_min", format!("temp_min {b:.0}°C ≤ required {lo:.0}°C"), b <= lo + 1e-9),
            None => unchecked(&mut g, "temp_min", "declares no temp_min — UNCHECKED, not a pass".into(), "attribute temp_min = <datasheet>degC;"),
        }
    }
    if let Some(hi) = req.temp_max_c {
        match p.temp_max_c {
            Some(b) => push(&mut g, "temp_max", format!("temp_max {b:.0}°C ≥ required {hi:.0}°C"), b + 1e-9 >= hi),
            // no ambient rating: a junction-rated part meets the ambient
            // requirement THERMALLY when θ_JA, T_J,max and the dissipation
            // are all known — T_J = T_A,max + P·θ_JA ≤ T_J,max
            None => match (p.theta_ja_c_per_w, p.tj_max_c, req.p_diss_w) {
                (Some(theta), Some(tj_max), Some(pw)) => {
                    let tj = hi + pw * theta;
                    push(
                        &mut g,
                        "temp_max",
                        format!("no ambient rating — thermal: T_J = {hi:.0}°C + {pw:.3}W × {theta:.1}°C/W = {tj:.1}°C ≤ T_J,max {tj_max:.0}°C"),
                        tj <= tj_max + 1e-9,
                    );
                }
                (theta, tj, pw) => {
                    let mut missing = Vec::new();
                    if theta.is_none() { missing.push("theta_ja"); }
                    if tj.is_none() { missing.push("tj_max"); }
                    if pw.is_none() { missing.push("the stage's dissipation"); }
                    unchecked(
                        &mut g,
                        "temp_max",
                        format!("declares no ambient temp_max; thermal derivation (T_A + P·θ_JA ≤ T_J,max) needs {} — UNCHECKED, not a pass", missing.join(", ")),
                        "attribute temp_max = <datasheet ambient>degC; or attribute theta_ja = <°C/W>; + tj_max",
                    );
                }
            },
        }
    }
    if let Some(req_asil) = req.asil {
        match p.asil_capable {
            Some(b) => push(&mut g, "asil", format!("asil_capable {} ≥ required ASIL {}", b.as_str(), req_asil.as_str()), b >= req_asil),
            None => unchecked(&mut g, "asil", format!("declares no asil_capable (required ASIL {}) — UNCHECKED, not a pass", req_asil.as_str()), "attribute asil_capable = \"<vendor SEooC / FS-compliant claim>\";"),
        }
    }
    // protection functions: a clamp must clamp at or below the asked
    // ceiling; a cutoff must open at or below it; a lockout must hold
    // off at or above the asked floor; reverse-polarity must be declared
    if let Some(v) = req.ov_clamp_v {
        match p.ov_clamp_v {
            Some(b) => push(&mut g, "ov_clamp", format!("clamps at {b:.1}V ≤ required ceiling {v:.1}V"), b <= v + 1e-9),
            None => unchecked(&mut g, "ov_clamp", "declares no ov_clamp (no passive clamp) — UNCHECKED, not a pass".into(), "attribute ov_clamp = <clamp voltage>;"),
        }
    }
    if let Some(v) = req.ov_trip_v {
        match p.ov_trip_v {
            Some(b) => push(&mut g, "ov_trip", format!("cuts off at {b:.1}V ≤ required {v:.1}V"), b <= v + 1e-9),
            None => unchecked(&mut g, "ov_trip", "declares no ov_trip (no active overvoltage cutoff) — UNCHECKED, not a pass".into(), "attribute ov_trip = <cutoff voltage>;"),
        }
    }
    if let Some(v) = req.uv_trip_v {
        match p.uv_trip_v {
            Some(b) => push(&mut g, "uv_trip", format!("locks out below {b:.1}V ≥ required {v:.1}V"), b + 1e-9 >= v),
            None => unchecked(&mut g, "uv_trip", "declares no uv_trip (no undervoltage lockout) — UNCHECKED, not a pass".into(), "attribute uv_trip = <lockout voltage>;"),
        }
    }
    if req.reverse_polarity == Some(true) {
        match p.reverse_polarity {
            Some(b) => push(&mut g, "reverse_polarity", format!("reverse-polarity protection declared {b}"), b),
            None => unchecked(&mut g, "reverse_polarity", "declares no reverse_polarity protection — UNCHECKED, not a pass".into(), "attribute reverse_polarity = true;"),
        }
    }
    if let Some(q) = &req.qual {
        match &p.qualification {
            Some(have) => push(&mut g, "qual", format!("qualification \"{have}\" covers required \"{q}\""), have.to_ascii_lowercase().contains(&q.to_ascii_lowercase())),
            None => unchecked(&mut g, "qual", format!("declares no qualification (required \"{q}\") — UNCHECKED, not a pass"), "attribute qualification = \"<e.g. AEC-Q100>\";"),
        }
    }
    g
}

/// The stage's dissipation at an operating point, from the block's
/// class and promises:
/// - linear → (Vin − Vout)·I + Vin·Iq;
/// - switching → the SAME physics loss model the `supply` chooser rates
///   candidates with: I²·R_ds·D + Vin·I·f_sw·t_sw + Vin·Iq (conduction
///   at the duty cycle + switching transitions + quiescent), when the
///   part declares rds_on/f_sw/t_sw; else the declared-efficiency
///   fallback (1 − η)/η · Vout · I, which lumps every loss into η;
/// - pass-through (prereg) → I²·R_on when rds_on is declared.
/// `None` when the figures to compute it are not declared.
#[allow(clippy::too_many_arguments)]
pub fn estimate_dissipation_w(
    class: &str,
    vin_v: Option<f64>,
    vout_v: Option<f64>,
    i_a: Option<f64>,
    i_q_a: Option<f64>,
    efficiency: Option<f64>,
    rds_on_ohm: Option<f64>,
    f_sw_hz: Option<f64>,
    t_sw_s: Option<f64>,
) -> Option<f64> {
    let i = i_a?;
    match class {
        "ldo" | "voltage_regulator" => {
            let (vin, vout) = (vin_v?, vout_v?);
            Some((vin - vout).max(0.0) * i + vin * i_q_a.unwrap_or(0.0))
        }
        "switching_regulator" => {
            // physics loss model first (the chooser's form), when declared.
            // BOOST (vout > vin): the switch carries the INPUT current
            // I_in = I·V_out/V_in at duty D = 1 − V_in/V_out, and the
            // transitions swing V_out. BUCK: conduction at D = V_out/V_in.
            if let (Some(vin), Some(vout), Some(rds), Some(f_sw), Some(t_sw)) =
                (vin_v, vout_v, rds_on_ohm, f_sw_hz, t_sw_s)
            {
                if vin > 0.0 && vout > 0.0 {
                    if vout > vin {
                        let i_in = i * vout / vin;
                        let duty = 1.0 - vin / vout;
                        return Some(i_in * i_in * rds * duty + vout * i_in * f_sw * t_sw + vin * i_q_a.unwrap_or(0.0));
                    }
                    let duty = vout / vin;
                    return Some(i * i * rds * duty + vin * i * f_sw * t_sw + vin * i_q_a.unwrap_or(0.0));
                }
            }
            let (vout, eta) = (vout_v?, efficiency?);
            if eta <= 0.0 { return None; }
            Some((1.0 - eta) / eta * vout * i)
        }
        "protection" => Some(i * i * rds_on_ohm?),
        _ => None,
    }
}

// ── shared text parsers (both callers read attribute / requirement text) ──

/// SI value text → base unit (`2.4A`, `150mA`, `30uV`, `3.5V`, `85%` → 0.85).
pub fn parse_si(v: &str) -> Option<f64> {
    let t = v.trim().trim_matches('"').trim_end_matches("rms").trim_end_matches("RMS").trim();
    let end = t
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    let num: f64 = t[..end].parse().ok()?;
    let unit = t[end..].trim();
    if unit == "%" {
        return Some(num / 100.0);
    }
    if matches!(unit, "°C/W" | "C/W" | "K/W" | "degC/W") {
        return Some(num);
    }
    let prefix = unit
        .trim_end_matches("Hz")
        .trim_end_matches("ohm")
        .trim_end_matches(['V', 'A', 'W', 'F', 'H', 's', 'Ω']);
    let scale = match prefix {
        "" => 1.0,
        "p" => 1e-12,
        "n" => 1e-9,
        "u" | "µ" | "μ" => 1e-6,
        "m" => 1e-3,
        "k" | "K" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        _ => return None,
    };
    Some(num * scale)
}

/// `-40degC` / `85°C` / `125C` / bare number → °C.
pub fn parse_temp_c(v: &str) -> Option<f64> {
    let t = v.trim().trim_matches('"');
    let end = t
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    let num: f64 = t[..end].parse().ok()?;
    match t[end..].trim() {
        "" | "C" | "°C" | "degC" => Some(num),
        _ => None,
    }
}

/// Efficiency text: `92%` → 0.92, `0.92` → 0.92, `92` → 0.92.
pub fn parse_efficiency(v: &str) -> Option<f64> {
    let t = v.trim().trim_matches('"');
    if let Some(p) = t.strip_suffix('%') {
        return p.trim().parse::<f64>().ok().map(|x| x / 100.0);
    }
    let x: f64 = t.parse().ok()?;
    Some(if x > 1.0 { x / 100.0 } else { x })
}

/// A requirement from `k=v, k=v` text (the requirement vocabulary of
/// bhdl-stdlib/power/stages.bhdl) — what the resolver stamps as
/// `stage_requirement` and what it parses from the source.
pub fn requirement_from_pairs<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> StageRequirement {
    let mut r = StageRequirement::default();
    for (k, v) in pairs {
        match k.trim() {
            "i_max" => r.i_max_a = parse_si(v),
            "vin" => r.vin_v = parse_si(v),
            "vin_min" => r.vin_min_v = parse_si(v),
            "vin_max" => r.vin_max_v = parse_si(v),
            "noise" => r.noise_v = parse_si(v),
            "efficiency_min" => r.efficiency_min = parse_efficiency(v),
            "phases" => r.phases = v.trim().parse::<f64>().ok(),
            "temp_min" => r.temp_min_c = parse_temp_c(v),
            "temp_max" => r.temp_max_c = parse_temp_c(v),
            "qual" => r.qual = Some(v.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()),
            "ov_clamp" => r.ov_clamp_v = parse_si(v),
            "ov_trip" => r.ov_trip_v = parse_si(v),
            "uv_trip" => r.uv_trip_v = parse_si(v),
            "reverse_polarity" => r.reverse_polarity = Some(v.trim().trim_matches('"').eq_ignore_ascii_case("true")),
            "asil" => r.asil = Asil::parse(v),
            _ => {}
        }
    }
    r
}

/// Promises from an attribute map (entity text or instance attributes).
/// `resolve` maps a raw attribute value to its final text (a caller that
/// reads entity text resolves `attribute f_sw = f_sw` through params;
/// an instance's attributes are already resolved).
pub fn promises_from_attrs<'a>(get: impl Fn(&str) -> Option<String>) -> StagePromises {
    StagePromises {
        output_current_a: get("output_current")
            .or_else(|| get("max_current"))
            .or_else(|| get("i_rating"))
            .and_then(|v| parse_si(&v)),
        vin_min_v: get("vin_min").or_else(|| get("input_voltage_min")).and_then(|v| parse_si(&v)),
        vin_max_v: get("vin_max").or_else(|| get("input_voltage_max")).and_then(|v| parse_si(&v)),
        output_noise_v: get("output_noise").and_then(|v| parse_si(&v)),
        efficiency: get("efficiency").and_then(|v| parse_efficiency(&v)),
        phases: get("phases").and_then(|v| v.trim().parse::<f64>().ok()),
        temp_min_c: get("temp_min").and_then(|v| parse_temp_c(&v)),
        temp_max_c: get("temp_max").and_then(|v| parse_temp_c(&v)),
        qualification: get("qualification").map(|v| v.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()),
        ov_clamp_v: get("ov_clamp").and_then(|v| parse_si(&v)),
        ov_trip_v: get("ov_trip").and_then(|v| parse_si(&v)),
        uv_trip_v: get("uv_trip").and_then(|v| parse_si(&v)),
        reverse_polarity: get("reverse_polarity").map(|v| v.trim().trim_matches('"').eq_ignore_ascii_case("true")),
        asil_capable: get("asil_capable").and_then(|v| Asil::parse(&v)),
        theta_ja_c_per_w: get("theta_ja").and_then(|v| parse_si(&v)),
        tj_max_c: get("tj_max").and_then(|v| parse_temp_c(&v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_follow_the_requirement_and_state_unchecked() {
        let req = requirement_from_pairs([("i_max", "2A"), ("vin", "12V"), ("noise", "30uV"), ("temp_max", "85degC")].into_iter());
        let p = promises_from_attrs(|k| match k {
            "output_current" => Some("3A".into()),
            "vin_min" => Some("3.5V".into()),
            "vin_max" => Some("28V".into()),
            "temp_max" => Some("85degC".into()),
            _ => None,
        });
        let g = check(&req, &p);
        let by = |n: &str| g.iter().find(|x| x.name == n).unwrap();
        assert!(by("i_max").ok() && by("vin_min").ok() && by("vin_max").ok() && by("temp_max").ok());
        assert!(by("noise").unchecked());
        assert!(g.iter().all(|x| x.name != "efficiency" && x.name != "qual"), "gates only for what was asked");
        // derate: 2.6A load needs 3.25A rating → 3A fails
        let req2 = requirement_from_pairs([("i_max", "2.6A")].into_iter());
        assert!(!check(&req2, &p)[0].ok());
        assert_eq!(parse_efficiency("92%"), Some(0.92));
        assert_eq!(parse_temp_c("-40degC"), Some(-40.0));
    }
}
