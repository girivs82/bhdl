//! The PROFESSIONAL power-delivery report (`bhdl pdreport`) — the
//! document that convinces a power engineer the tool did its job:
//! the topology, every selection with its survey and near-misses, the
//! sizing numbers the datasheet procedures produced, the simulated
//! CURVES (V(t) from the PWL engine, rendered as inline SVG), the
//! power-up/power-down/sleep timelines with their verified windows,
//! the decap networks, and the final sanity verdicts. Every number in
//! it is one the pipeline computed or cited — nothing decorative.

use std::collections::HashMap;

use bhdl_ast::SourceFile;
use bhdl_netlist::netlist::Netlist;
use bhdl_netlist::types::{InstanceId, NetId};

use crate::powerup::{PowerdownReport, PowerupReport, RailWave, Sev};
use crate::stage_resolution::StageResolution;

const PALETTE: [&str; 6] = ["#0057b7", "#c1272d", "#1a7f37", "#8a4fbe", "#b8860b", "#0e7c86"];

/// One scenario's V(t) chart as inline SVG (640×260, ms on X).
fn svg_chart(title: &str, waves: &[RailWave]) -> String {
    let (w, h, ml, mb, mt, mr) = (640.0, 260.0, 52.0, 34.0, 26.0, 110.0);
    let t0 = waves
        .iter()
        .flat_map(|wv| wv.points.first().map(|p| p.0))
        .fold(f64::INFINITY, f64::min);
    let t1 = waves
        .iter()
        .flat_map(|wv| wv.points.last().map(|p| p.0))
        .fold(0.0f64, f64::max);
    let vmax = waves
        .iter()
        .flat_map(|wv| wv.points.iter().map(|p| p.1).chain([wv.v_nom]))
        .fold(0.0f64, f64::max)
        * 1.1
        + 1e-9;
    if !(t1 > t0) || waves.is_empty() {
        return String::new();
    }
    let x = |t: f64| ml + (t - t0) / (t1 - t0) * (w - ml - mr);
    let y = |v: f64| h - mb - v / vmax * (h - mb - mt);
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n\
         <rect width=\"{w}\" height=\"{h}\" fill=\"white\" stroke=\"#ccc\"/>\n\
         <text x=\"{ml}\" y=\"17\" font-family=\"sans-serif\" font-size=\"13\" font-weight=\"bold\">{title}</text>\n"
    );
    // axes + gridlines
    for k in 0..=4 {
        let v = vmax * k as f64 / 4.0;
        let yy = y(v);
        s.push_str(&format!(
            "<line x1=\"{ml}\" y1=\"{yy:.1}\" x2=\"{:.1}\" y2=\"{yy:.1}\" stroke=\"#eee\"/>\n\
             <text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"10\" text-anchor=\"end\">{v:.2}V</text>\n",
            w - mr, ml - 4.0, yy + 3.0
        ));
        let t = t0 + (t1 - t0) * k as f64 / 4.0;
        let xx = x(t);
        s.push_str(&format!(
            "<line x1=\"{xx:.1}\" y1=\"{mt}\" x2=\"{xx:.1}\" y2=\"{:.1}\" stroke=\"#eee\"/>\n\
             <text x=\"{xx:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"10\" text-anchor=\"middle\">{:.1}ms</text>\n",
            h - mb, h - mb + 14.0, t * 1e3
        ));
    }
    for (i, wv) in waves.iter().enumerate() {
        let color = PALETTE[i % PALETTE.len()];
        let pts: String = wv
            .points
            .iter()
            .map(|(t, v)| format!("{:.1},{:.1}", x(*t), y(*v)))
            .collect::<Vec<_>>()
            .join(" ");
        s.push_str(&format!(
            "<polyline points=\"{pts}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1.6\"/>\n"
        ));
        // nominal marker + legend
        let yn = y(wv.v_nom);
        s.push_str(&format!(
            "<line x1=\"{ml}\" y1=\"{yn:.1}\" x2=\"{:.1}\" y2=\"{yn:.1}\" stroke=\"{color}\" stroke-dasharray=\"3 4\" opacity=\"0.4\"/>\n",
            w - mr
        ));
        let ly = mt + 14.0 * i as f64 + 8.0;
        s.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{ly:.1}\" x2=\"{:.1}\" y2=\"{ly:.1}\" stroke=\"{color}\" stroke-width=\"2\"/>\n\
             <text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"11\">{} ({:.2}V)</text>\n",
            w - mr + 6.0, w - mr + 26.0, w - mr + 30.0, ly + 3.5, wv.rail, wv.v_nom
        ));
    }
    s.push_str("</svg>\n");
    s
}


/// Bare numeric values (an un-snapped computed child) get an SI
/// prefix; values that already carry a unit pass through.
fn si_fmt(v: &str) -> String {
    let t = v.trim();
    let Ok(x) = t.parse::<f64>() else { return t.to_string() };
    let ax = x.abs();
    let (m, p) = if ax >= 1e6 {
        (1e6, "M")
    } else if ax >= 1e3 {
        (1e3, "k")
    } else if ax >= 1.0 || ax == 0.0 {
        (1.0, "")
    } else if ax >= 1e-3 {
        (1e-3, "m")
    } else if ax >= 1e-6 {
        (1e-6, "µ")
    } else if ax >= 1e-9 {
        (1e-9, "n")
    } else {
        (1e-12, "p")
    };
    let n = x / m;
    if (n - n.round()).abs() < 1e-6 {
        format!("{:.0}{p}", n)
    } else {
        format!("{:.2}{p}", n)
    }
}

fn findings_md(out: &mut String, findings: &[crate::powerup::Finding]) {
    for f in findings {
        let tag = match f.sev {
            Sev::Error => "❌",
            Sev::Warning => "⚠️",
            Sev::Info => "ℹ️",
        };
        out.push_str(&format!("- {tag} {}\n", f.text));
    }
    if findings.is_empty() {
        out.push_str("- ✅ every declared window and bound holds\n");
    }
}

/// Assemble the report. Everything passed in was computed by the same
/// pipeline the build runs — the report renders, it never re-derives.
pub fn render(
    board: &str,
    netlist: &Netlist,
    _sf: &SourceFile,
    resolutions: &[StageResolution],
    aggregation: &[String],
    up: &PowerupReport,
    down: &PowerdownReport,
    sanity: &[String],
    signoff: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Power-Delivery Report — {board}\n\nGenerated by `bhdl pdreport`. Every number below was computed by the\nbuild pipeline or cited from a datasheet — the same gates that run on\nevery build produced them (nothing here is decorative).\n\n"
    ));

    // ── indexes ──
    let mut pin_net: HashMap<(InstanceId, String), NetId> = HashMap::new();
    for pi in netlist.pin_instances.values() {
        let (Some(net), Some(p)) = (pi.net, netlist.pins.get(pi.pin_def)) else { continue };
        pin_net.insert((pi.instance, p.name.clone()), net);
    }
    let attr = |i: InstanceId, k: &str| -> Option<String> {
        netlist.instances.get(i).and_then(|x| x.attributes.get(k).cloned())
    };
    let net_name = |n: NetId| -> String {
        netlist.nets.get(n).and_then(|x| x.name.clone()).unwrap_or_else(|| "<net>".into())
    };
    let module_of = |i: InstanceId| -> String {
        netlist
            .modules
            .get(netlist.instances.get(i).map(|x| x.definition).unwrap_or_default())
            .map(|m| m.name.clone())
            .unwrap_or_default()
    };

    // ── 1. topology ──
    out.push_str("## 1. Power topology\n\n| Stage | Block | Topology | Feed | Rail | V_out | Key figures |\n|---|---|---|---|---|---|---|\n");
    let mut stages: Vec<(InstanceId, String)> = netlist
        .instances
        .iter()
        .filter(|(i, x)| {
            x.attributes.contains_key("output_voltage") && pin_net.contains_key(&(*i, "VOUT".to_string()))
        })
        .map(|(i, x)| (i, x.name.clone()))
        .collect();
    stages.sort_by(|a, b| a.1.cmp(&b.1));
    for (i, name) in &stages {
        let feed = pin_net.get(&(*i, "VIN".to_string())).map(|n| net_name(*n)).unwrap_or_default();
        let rail = pin_net.get(&(*i, "VOUT".to_string())).map(|n| net_name(*n)).unwrap_or_default();
        let figs: Vec<String> = [
            ("f_sw", "fSW"),
            ("rds_on", "RDS(on)"),
            ("i_sw_avg_limit", "I_lim(avg)"),
            ("i_valley_limit", "I_lim(valley)"),
            ("theta_ja", "θJA"),
            ("efficiency", "η"),
        ]
        .iter()
        .filter_map(|(k, lbl)| attr(*i, k).map(|v| format!("{lbl}={}", v.trim_matches('"'))))
        .collect();
        out.push_str(&format!(
            "| {name} | {} | {} | {feed} | {rail} | {} | {} |\n",
            module_of(*i),
            attr(*i, "topology").map(|t| t.trim_matches('"').to_string()).unwrap_or_default(),
            attr(*i, "output_voltage").unwrap_or_default(),
            figs.join(", ")
        ));
    }

    // ── 2. selections (the surveys, verbatim) ──
    out.push_str("\n## 2. Requirement resolution — every candidate, every gate\n\n");
    if resolutions.is_empty() {
        out.push_str("No stage requirements on this board (stages are hand-instantiated blocks).\n");
    }
    for r in resolutions {
        out.push_str("```\n");
        out.push_str(&crate::stage_resolution::render_report(r));
        out.push_str("```\n\n");
    }
    if !aggregation.is_empty() {
        out.push_str("### Multi-output (PMIC) aggregation\n\n");
        for l in aggregation {
            out.push_str(&format!("{l}\n"));
        }
        out.push('\n');
    }

    // ── 3. sizing — the datasheet procedures' outputs ──
    out.push_str("## 3. Sizing — what the design procedures produced\n\nPer stage: the application-circuit values its block's `design { }`\ncomputed from the datasheet equations (dividers, inductors, banks).\nThe symbolic derivations live in the block sources; the full stress\nsign-off table is `bhdl report`.\n\n| Stage | Child | Value |\n|---|---|---|\n");
    for (_i, name) in &stages {
        let prefix = format!("{name}_");
        let mut kids: Vec<(String, String)> = netlist
            .instances
            .iter()
            .filter(|(_, x)| x.name.starts_with(&prefix))
            .filter_map(|(k, x)| {
                netlist
                    .instances
                    .get(k)
                    .and_then(|xx| xx.attributes.get("value").cloned())
                    .map(|v| (x.name.clone(), v))
            })
            .collect();
        kids.sort();
        for (kname, v) in kids {
            out.push_str(&format!("| {name} | {kname} | {} |\n", si_fmt(&v)));
        }
    }

    // ── 4. curves ──
    out.push_str("\n## 4. Simulated curves (piecewise-linear event engine)\n\nModeling choices, stated:\n");
    for n in &up.notes {
        out.push_str(&format!("- {n}\n"));
    }
    out.push('\n');
    for (label, waves) in up.waves.iter().chain(down.waves.iter()) {
        let chart = svg_chart(label, waves);
        if !chart.is_empty() {
            out.push_str(&chart);
            out.push('\n');
        }
    }

    // ── 5. power-up ──
    out.push_str("\n## 5. Power-up timeline and windows\n\n| t | event |\n|---|---|\n");
    for e in &up.events {
        out.push_str(&format!("| {:.3} ms | {} |\n", e.t * 1e3, e.text));
    }
    out.push_str("\nRails:\n\n| Rail | V_nom | t_good | Sags |\n|---|---|---|---|\n");
    for r in &up.rails {
        out.push_str(&format!(
            "| {} | {:.2} V | {} | {} |\n",
            r.net,
            r.v_nom,
            r.t_good.map(|t| format!("{:.3} ms", t * 1e3)).unwrap_or_else(|| "NEVER".into()),
            if r.sags.is_empty() { "—".into() } else {
                r.sags.iter().map(|(a, b, vm)| format!("{:.3}–{:.3} ms (min {:.2} V)", a * 1e3, b * 1e3, vm)).collect::<Vec<_>>().join("; ")
            }
        ));
    }
    if !up.steps.is_empty() {
        out.push_str("\nLoad steps (each fired alone from the settled point):\n\n| Domain | Rail | Self-droop | Verdict | Extra stage demand |\n|---|---|---|---|---|\n");
        for st in &up.steps {
            out.push_str(&format!(
                "| {}.{} | {} | {:.0} mV | {} | {} |\n",
                st.owner, st.domain, st.rail, st.self_droop_v * 1e3,
                match st.droop_ok { Some(true) => "within droop_max", Some(false) => "EXCEEDS droop_max", None => "no droop_max declared (stated)" },
                st.extra_demand_a.iter().map(|(n, a)| format!("{n} +{a:.2}A")).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    if !up.interactions.is_empty() {
        out.push_str("\nInteraction screen (peak-aligned superposition + self-consistency):\n\n");
        for l in &up.interactions {
            out.push_str(&format!("- {l}\n"));
        }
    }
    out.push_str("\nFindings:\n\n");
    findings_md(&mut out, &up.findings);

    // ── 6. power-down / sleep ──
    out.push_str("\n## 6. Power-down and sleep\n\n### Input loss\n\n| t | event |\n|---|---|\n");
    for e in &down.input_loss {
        out.push_str(&format!("| {:.3} ms | {} |\n", e.t * 1e3, e.text));
    }
    if !down.sleep.is_empty() {
        out.push_str("\n### Sleep entry\n\n| t | event |\n|---|---|\n");
        for e in &down.sleep {
            out.push_str(&format!("| {:.3} ms | {} |\n", e.t * 1e3, e.text));
        }
    }
    out.push_str("\nFindings:\n\n");
    findings_md(&mut out, &down.findings);

    // ── 7. decap networks ──
    if let Some(reports) = netlist.get_analysis_data().map(|a| &a.decap_reports) {
        if !reports.is_empty() {
            out.push_str("\n## 7. Decap networks (Z(f) mask synthesis)\n\n");
            for r in reports {
                out.push_str(&format!(
                    "### {} on {} (library {}, mask {} breakpoints, z_margin {}%)\n\n",
                    r.target, r.net, r.lib, r.mask_breakpoints, r.z_margin_pct
                ));
                for st in &r.steps {
                    out.push_str(&format!(
                        "- {} = {} ({}) → worst |Z|/mask {:.2} at {:.2} MHz\n",
                        st.instance, st.value, st.entity, st.ratio_after, st.freq_hz / 1e6
                    ));
                }
                for m in &r.margin_added {
                    out.push_str(&format!("- margin: {m}\n"));
                }
                out.push_str(&format!(
                    "- final worst |Z|/mask {:.2} at {:.2} MHz; {} single-open(s) verified, {} bulk exempt (stated)\n\n",
                    r.final_ratio, r.final_freq_hz / 1e6, r.opens_verified, r.opens_bulk_exempt
                ));
            }
        }
    }

    // ── 7.5 the solved stress sign-off (the margins table) ──
    match signoff {
        Some(txt) => {
            out.push_str("\n## 7.5 Stress sign-off — solved margins per part\n\nThe same DC solve and margin computation `bhdl report --simulate`\nruns (rating ÷ derated stress per axis; junction rows compose\nP·θJA against the datasheet T_J):\n\n```\n");
            out.push_str(txt);
            out.push_str("```\n");
        }
        None => {
            out.push_str("\n## 7.5 Stress sign-off\n\n- ⚠ the DC solve did not converge for this board — margins UNCOMPUTED (stated, never silent); run `bhdl report --simulate` after fixing the operating point\n");
        }
    }

    // ── 8. final sanity ──
    out.push_str("\n## 8. Final PDN sanity (loop stability, resonance)\n\n");
    if sanity.is_empty() {
        out.push_str("- ✅ nothing to state: every stage inside its declared envelope, no uncharacterized capacitance on swept rails\n");
    }
    for l in sanity {
        out.push_str(&format!("- {l}\n"));
    }
    out.push_str("\n---\n*Sequencing mechanisms are verified by ERC033 on every build.*\n");
    out
}
