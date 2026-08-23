#[test]
fn inductor_plus_cap_dc_node_voltage_present() {
    use bhdl_spice::circuit::Circuit;
    use std::collections::HashMap;
    let mut c = Circuit::new();
    c.add_node("vcc".into(), None);
    c.add_node("sw".into(), None);
    c.add_node("out".into(), None);
    c.add_node("0".into(), None);
    c.add_branch("V1".into(), "vcc", "0", "VoltageSource".into(), 12.0, None);
    c.add_branch("R1".into(), "vcc", "sw", "Resistor".into(), 1.0, None);
    c.add_branch("L1".into(), "sw", "out", "Inductor".into(), 100e-6, None);
    c.add_branch("R2".into(), "out", "0", "Resistor".into(), 100.0, None);
    c.add_branch("C1".into(), "out", "0", "Capacitor".into(), 100e-9, None);
    let (r, cref) = bhdl_spice::input_draw::solve_dc_with_input_draws(c, &HashMap::new()).expect("solve");
    let mut seen = Vec::new();
    for (idx, v) in &r.node_voltages {
        seen.push((cref.get_node_name(*idx).unwrap_or("?").to_string(), *v));
    }
    seen.sort_by(|a, b| a.0.cmp(&b.0));
    println!("{seen:?}");
    assert!(seen.iter().any(|(n, _)| n == "out"), "OUT missing: {seen:?}");
}
