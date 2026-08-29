//! FIT-provider plugin (the component-DB provider pattern applied to
//! reliability): a standard whose data is registration-gated or
//! proprietary (FIDES, an OEM handbook, a paid tool) plugs in as an
//! EXECUTABLE — protocol v1, JSON stdin→stdout. The mock provider
//! proves: the full request (standard, class, inputs, the WHOLE
//! mission with phases) reaches the plugin, the returned λ/basis/
//! source land verbatim on the part, and a refusing provider leaves
//! the usual NAMED gap that tells the customer how to plug in.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn board() -> String {
    let src = std::fs::read_to_string(workspace_root().join("tests/circuits/realistic/test_safety_dfa.bhdl")).unwrap();
    let out = src
        .replace(
            "board DfaDemo {",
            r#"entity CustomAsic() {
    pin 1: signal inout;
    pin 2: ground;
    attribute component_class = "asic";
    safety {
        handbook class="asic_custom" per="FIDES" source="customer reliability flow";
    }
}

board DfaDemo {"#,
        )
        .replace(
            "    @V33A -> mon_top: Res(10kΩ).1;",
            "    @V33A -> ca: CustomAsic().1; ca.2 -> @GND;\n    @V33A -> mon_top: Res(10kΩ).1;",
        )
        .replace(
            "mission { ambient = 40degC; lifetime = 15000h; }",
            "mission { profile = passenger_compartment; lifetime = 15000h; }",
        );
    assert_ne!(out, src, "fixture shape changed — update the replaces");
    out
}

#[test]
fn provider_plugin_answers_and_refuses_honestly() {
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_fit_provider_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("fp.bhdl");
    std::fs::write(&f, board()).unwrap();

    // mock provider: records the request, answers a sourced λ
    let req_dump = dir.join("request.json");
    let provider = dir.join("mock-provider.sh");
    std::fs::write(
        &provider,
        format!(
            "#!/bin/sh\ncat > {}\necho '{{\"fit\": 42.5, \"basis\": \"λ=42.5 FIT = base·π_thermal·π_TCy over the mission phases (mock)\", \"source\": \"FIDES 2022-A (mock provider)\"}}'\n",
            req_dump.display()
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&provider).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o755);
    std::fs::set_permissions(&provider, perms).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root)
        .env("BHDL_FIT_PROVIDER", &provider)
        .arg("-I")
        .arg(&root)
        .arg(&f)
        .arg("safety");
    let out = cmd.output().expect("spawn");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    // the λ, basis and source land verbatim on the part
    assert!(
        text.contains("λ=42.5 FIT") && text.contains("[provider: FIDES 2022-A (mock provider)]"),
        "provider result not applied:\n{}",
        text.lines().filter(|l| l.contains("ca") || l.contains("42.5")).collect::<Vec<_>>().join("\n")
    );
    // the FULL request reached the plugin: standard, class, and the
    // whole mission with its phases (the provider owns its own
    // per-phase composition)
    let req = std::fs::read_to_string(&req_dump).expect("provider never saw a request");
    for needle in ["\"protocol\":1", "\"standard\":\"FIDES\"", "\"class\":\"asic_custom\"", "\"phases\"", "drive_hot"] {
        assert!(req.contains(needle), "request missing {needle}:\n{req}");
    }

    // a refusing provider: the part keeps the NAMED gap telling the
    // customer exactly how to plug in — never a default
    let refuser = dir.join("refuser.sh");
    std::fs::write(&refuser, "#!/bin/sh\ncat > /dev/null\necho '{\"error\": \"no FIDES license on this machine\"}'\n").unwrap();
    let mut perms = std::fs::metadata(&refuser).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&refuser, perms).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root)
        .env("BHDL_FIT_PROVIDER", &refuser)
        .arg("-I")
        .arg(&root)
        .arg(&f)
        .arg("safety");
    let out = cmd.output().expect("spawn");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(
        text.contains("no coefficient table for 'FIDES' and no provider answered")
            && text.contains("bhdl-fit-provider-fides"),
        "refusal gap not named:\n{}",
        text.lines().filter(|l| l.contains("FIDES")).collect::<Vec<_>>().join("\n")
    );
}
