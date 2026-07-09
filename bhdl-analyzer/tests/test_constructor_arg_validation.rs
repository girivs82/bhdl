//! Constructor-argument validation: a named arg binding to no declared
//! parameter (or an excess positional arg) is an Error — such args used to
//! pass through as dead instance attributes, silently swallowing intent.

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::{extract_entity_param_names, validate_constructor_args};
use bhdl_common::DiagnosticKind;

fn source_file(src: &str) -> SourceFile {
    let parsed = parse(src);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());
    SourceFile::cast(parsed.syntax()).unwrap()
}

const RES: &str = r#"
entity Res(value: resistance, tolerance: percentage = 5%, wattage: power = 0.25W) {
    pin 1: signal inout;
    pin 2: signal inout;
    attribute resistance = value;
}
alias Resistor = Res;
"#;

#[test]
fn accepts_declared_named_and_positional_args() {
    let sf = source_file(&format!(
        "{RES}\nboard B {{ ground GND; @A -> r: Res(2.5Ohm, wattage=10W).1; r.2 -> @GND; }}"
    ));
    let params = extract_entity_param_names(&sf);
    let v = validate_constructor_args(&sf, &params);
    assert!(v.is_empty(), "unexpected: {:?}", v.iter().map(|d| &d.message).collect::<Vec<_>>());
}

#[test]
fn rejects_unknown_named_arg_with_suggestion() {
    let sf = source_file(&format!(
        "{RES}\nboard B {{ ground GND; @A -> r: Res(1k, tolernce=1%).1; r.2 -> @GND; }}"
    ));
    let params = extract_entity_param_names(&sf);
    let v = validate_constructor_args(&sf, &params);
    assert_eq!(v.len(), 1, "{:?}", v);
    match &v[0].kind {
        DiagnosticKind::UnknownConstructorArg { arg, entity, suggestions } => {
            assert_eq!(arg, "tolernce");
            assert_eq!(entity, "Res");
            assert!(suggestions.contains(&"tolerance".to_string()), "{suggestions:?}");
        }
        other => panic!("wrong kind: {other:?}"),
    }
}

#[test]
fn alias_inherits_target_params() {
    // `Resistor` is an alias of `Res` — validation resolves it.
    let sf = source_file(&format!(
        "{RES}\nboard B {{ ground GND; @A -> r: Resistor(1k, bogus=2).1; r.2 -> @GND; }}"
    ));
    let params = extract_entity_param_names(&sf);
    assert!(params.contains_key("Resistor"), "alias not indexed");
    let v = validate_constructor_args(&sf, &params);
    assert_eq!(v.len(), 1);
    assert!(v[0].message.contains("bogus"));
}

#[test]
fn reserved_synth_attrs_are_exempt() {
    // The supply desugarer / expansion interpreter stamp these; they are
    // sanctioned attribute passthrough, not unknown parameters.
    let sf = source_file(&format!(
        "{RES}\nboard B {{ ground GND; \
         @A -> r: Res(1k, supply_profile=\"cost\", i_supply=10mA, expansion_parent=\"u1\").1; \
         r.2 -> @GND; }}"
    ));
    let params = extract_entity_param_names(&sf);
    let v = validate_constructor_args(&sf, &params);
    assert!(v.is_empty(), "reserved attrs flagged: {:?}", v.iter().map(|d| &d.message).collect::<Vec<_>>());
}

#[test]
fn rejects_excess_positional_arg() {
    let sf = source_file(&format!(
        "{RES}\nboard B {{ ground GND; @A -> r: Res(1k, 1%, 0.5W, 7).1; r.2 -> @GND; }}"
    ));
    let params = extract_entity_param_names(&sf);
    let v = validate_constructor_args(&sf, &params);
    assert_eq!(v.len(), 1, "{:?}", v);
    assert!(v[0].message.contains("positional"), "{}", v[0].message);
}

#[test]
fn unknown_entity_not_guessed() {
    // An unindexed type is left to symbol resolution, never false-flagged.
    let sf = source_file(
        "board B { ground GND; @A -> r: Mystery(foo=1, bar=2).1; r.2 -> @GND; }"
    );
    let params = extract_entity_param_names(&sf);
    let v = validate_constructor_args(&sf, &params);
    assert!(v.is_empty(), "{:?}", v);
}
