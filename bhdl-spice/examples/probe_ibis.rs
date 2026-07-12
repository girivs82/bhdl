fn main() {
    for f in ["u3a33t44.ibs", "u3a33c44.ibs", "u3a43c00.ibs"] {
        let path = std::path::Path::new("vendor/ibis").join(f);
        match bhdl_spice::ibis::parse_file(&path) {
            Ok(ib) => {
                println!("── {f}: ver={} components={} models={} selectors={}",
                    ib.ibis_ver, ib.components.len(), ib.models.len(), ib.model_selectors.len());
                for c in &ib.components {
                    println!("   [Component] {} pins={}", c.name, c.pins.len());
                }
                for (name, m) in ib.models.iter().take(3) {
                    println!("   [Model] {name} type={} vcc={:?} pd={} pu={} gc={} pc={}",
                        m.model_type, m.vcc(bhdl_spice::ibis::Corner::Typ),
                        m.pulldown.as_ref().map(|t| t.typ.len()).unwrap_or(0),
                        m.pullup.as_ref().map(|t| t.typ.len()).unwrap_or(0),
                        m.gnd_clamp.as_ref().map(|t| t.typ.len()).unwrap_or(0),
                        m.power_clamp.as_ref().map(|t| t.typ.len()).unwrap_or(0));
                }
            }
            Err(e) => println!("── {f}: PARSE ERROR {e}"),
        }
    }
}
