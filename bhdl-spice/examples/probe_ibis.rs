//! Sweep every .ibs under vendor/ibis/ through the parser and report
//! per-file structure — the real-world conformance harness.
fn main() {
    let root = std::path::Path::new("vendor/ibis");
    let mut files: Vec<std::path::PathBuf> = walk(root);
    files.sort();
    let (mut ok, mut bad, mut empty_tables) = (0usize, 0usize, 0usize);
    for path in &files {
        match bhdl_spice::ibis::parse_file(path) {
            Ok(ib) => {
                let models = ib.models.len();
                let pins: usize = ib.components.iter().map(|c| c.pins.len()).sum();
                let tables: usize = ib.models.values().map(|m| {
                    [m.pulldown.as_ref(), m.pullup.as_ref(), m.gnd_clamp.as_ref(), m.power_clamp.as_ref()]
                        .iter().flatten().map(|t| t.typ.len()).sum::<usize>()
                }).sum();
                if models == 0 || pins == 0 || tables == 0 {
                    empty_tables += 1;
                    println!("HOLLOW {} comps={} pins={} models={} table_pts={}",
                        path.display(), ib.components.len(), pins, models, tables);
                } else { ok += 1; }
            }
            Err(e) => { bad += 1; println!("FAIL   {}: {e}", path.display()); }
        }
    }
    println!("── {} files: {} ok, {} hollow, {} failed", files.len(), ok, empty_tables, bad);
}
fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() { out.extend(walk(&p)); }
            else if p.extension().map(|x| x == "ibs").unwrap_or(false) { out.push(p); }
        }
    }
    out
}
