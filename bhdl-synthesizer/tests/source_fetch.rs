//! End-to-end test for level-3 source resolvers: a proprietary library
//! declared with `source = "<scheme>:<locator>", rev = …` is fetched
//! through an external `bhdl-source-<scheme>` helper, cached, and its
//! entity resolves — then a second resolve hits the cache with the
//! helper removed (proving offline-from-cache). See
//! docs/spec/Source_Resolvers.md.
//!
//! Single test per binary: it sets `$BHDL_CACHE` (process-global), so it
//! must not share a binary with parallel tests.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_common::library::LibraryResolver;
use bhdl_common::source::FetchOptions;
use bhdl_parser::parse;
use bhdl_synthesizer::import_loader::ImportLoader;
use std::path::{Path, PathBuf};

fn write_exec(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

const BOARD: &str = r#"
import { AcmeWidget } from "acme-stdlib/parts/widget.bhdl";
board Demo { power VCC = 3.3V @ 1A; ground GND; w: AcmeWidget(); }
"#;

#[test]
fn fetches_source_dep_via_helper_then_serves_from_cache() {
    let t = std::env::temp_dir().join(format!("bhdl_srcfetch_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&t);
    std::fs::create_dir_all(&t).unwrap();

    // Isolate the content cache for this test.
    std::env::set_var("BHDL_CACHE", t.join("cache"));

    // The "upstream" library content the helper will deliver.
    let upstream = t.join("upstream/acme-stdlib");
    std::fs::create_dir_all(upstream.join("parts")).unwrap();
    std::fs::write(
        upstream.join("manifest.toml"),
        "[library]\nname = \"acme-stdlib\"\nversion = \"2.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        upstream.join("parts/widget.bhdl"),
        "entity AcmeWidget { pin A: signal inout; }\n",
    )
    .unwrap();

    // A fixture resolver helper: reads the JSON request on stdin, pulls
    // out "dest" (compact JSON, no spaces), and copies the upstream lib
    // into it. Stands in for a real `bhdl-source-git` / `-p4`.
    let resolver_dir = t.join("resolvers");
    std::fs::create_dir_all(&resolver_dir).unwrap();
    let helper = resolver_dir.join("bhdl-source-test");
    write_exec(
        &helper,
        &format!(
            "#!/bin/sh\n\
             req=$(cat)\n\
             dest=$(printf '%s' \"$req\" | sed 's/.*\"dest\":\"\\([^\"]*\\)\".*/\\1/')\n\
             mkdir -p \"$dest\"\n\
             cp -R \"{}/.\" \"$dest/\"\n\
             printf '{{\"protocol\":1,\"ok\":true}}'\n",
            upstream.display()
        ),
    );

    // Manifest: a source dep using our fixture `test:` scheme.
    let mp = t.join("bhdl.toml");
    std::fs::write(
        &mp,
        "[project]\nname=\"demo\"\n\
         [libraries]\n\
         acme-stdlib = { source = \"test:acme\", rev = \"v2.1.0\", version = \"2.1\" }\n",
    )
    .unwrap();

    let resolver = LibraryResolver::new(Some(&mp), &[], None, None)
        .unwrap()
        .with_fetch_options(FetchOptions {
            resolver_dirs: vec![resolver_dir.clone()],
            offline: false,
        });

    // 1) First resolve: helper fetches → cache → entity loads.
    let load = |r: LibraryResolver| -> ImportLoader {
        let pr = parse(BOARD);
        assert!(pr.errors().is_empty(), "parse: {:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let mut loader = ImportLoader::new(".");
        loader.set_resolver(r);
        loader.process_imports(&sf).expect("process_imports (fetch)");
        loader
    };
    let loader = load(resolver.clone());
    assert!(loader.get_entity("AcmeWidget").is_some(), "AcmeWidget should resolve after fetch");

    // 2) Cache hit, helper removed: must still resolve from cache.
    std::fs::remove_file(&helper).unwrap();
    let resolver2 = LibraryResolver::new(Some(&mp), &[], None, None)
        .unwrap()
        .with_fetch_options(FetchOptions {
            resolver_dirs: vec![resolver_dir.clone()],
            offline: true, // belt-and-suspenders: cache only
        });
    let loader2 = load(resolver2);
    assert!(
        loader2.get_entity("AcmeWidget").is_some(),
        "AcmeWidget should resolve from cache with the helper removed + offline"
    );

    // 3) The lockfile records the source + rev + sha256 hash.
    let lock = resolver.compute_lockfile().unwrap();
    assert_eq!(lock.libraries.len(), 1);
    let l = &lock.libraries[0];
    assert_eq!(l.source, "test:acme");
    assert_eq!(l.rev.as_deref(), Some("v2.1.0"));
    assert!(l.hash.starts_with("sha256:"));

    let _ = std::fs::remove_dir_all(&t);
}

// Helper: LibraryResolver isn't Clone-by-derive used above? It is
// (derives Clone). Suppress unused if needed.
#[allow(dead_code)]
fn _assert_clone(r: &LibraryResolver) -> LibraryResolver {
    r.clone()
}

#[allow(dead_code)]
fn _unused(_p: PathBuf) {}
