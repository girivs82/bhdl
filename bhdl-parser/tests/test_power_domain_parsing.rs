//! Unit tests for power domain parsing (Phase 1: Scalability)

use bhdl_parser;

#[test]
fn test_basic_power_domain_parsing() {
    let code = r#"
board TestBoard {
    power_domain @VCC_3V3 = 3.3V @ 2A {
        sources {
            regulator: LM7805().OUT;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify POWER_DOMAIN_DEF exists
    let mut found_power_domain = false;
    for child in syntax.descendants() {
        if child.kind() == bhdl_parser::SyntaxKind::POWER_DOMAIN_DEF {
            found_power_domain = true;
            println!("Found POWER_DOMAIN_DEF!");
            break;
        }
    }

    assert!(found_power_domain, "POWER_DOMAIN_DEF not found in parsed syntax tree");
}

#[test]
fn test_power_domain_with_distribution() {
    let code = r#"
board TestBoard {
    power_domain @VCC_5V = 5V @ 3A {
        sources {
            usb: USB_5V_Connector().VBUS;
        }

        distribution {
            mcu.VDD;
            sensor.VCC;
            display.VDD;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify DISTRIBUTION_BLOCK exists
    let mut found_distribution = false;
    for child in syntax.descendants() {
        if child.kind() == bhdl_parser::SyntaxKind::DISTRIBUTION_BLOCK {
            found_distribution = true;
            println!("Found DISTRIBUTION_BLOCK!");
            break;
        }
    }

    assert!(found_distribution, "DISTRIBUTION_BLOCK not found");
}

#[test]
fn test_power_domain_with_range_expansion() {
    let code = r#"
board TestBoard {
    power_domain @VCC_1V8 = 1.8V @ 1.5A {
        sources {
            converter: TPS54302().OUT;
        }

        distribution {
            fpga.VCCO[0..7];
            memory[0..3].VDD;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify DOT_DOT token exists (for range syntax)
    let mut found_range = false;
    for token in syntax.descendants_with_tokens() {
        if let rowan::NodeOrToken::Token(t) = token {
            if t.kind() == bhdl_parser::SyntaxKind::DOT_DOT {
                found_range = true;
                println!("Found range operator '..'");
                break;
            }
        }
    }

    assert!(found_range, "Range operator '..' not found");
}

#[test]
fn test_power_domain_with_wildcard() {
    let code = r#"
board TestBoard {
    power_domain @VCC_CORE = 1.2V @ 5A {
        sources {
            pmic: TPS65950().VCORE;
        }

        distribution {
            cores[*].VDD;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify wildcard exists
    let text = syntax.text().to_string();
    assert!(text.contains("[*]"), "Wildcard '[*]' not found in parsed text");
}

#[test]
fn test_power_domain_with_decoupling() {
    let code = r#"
board TestBoard {
    power_domain @VCC_3V3 = 3.3V @ 2A {
        sources {
            reg: LM7805().OUT;
        }

        decoupling {
            near reg: 10µF @ 2, 100nF @ 4;
            distributed: 1µF @ 10;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify DECOUPLING_BLOCK exists
    let mut found_decoupling = false;
    for child in syntax.descendants() {
        if child.kind() == bhdl_parser::SyntaxKind::DECOUPLING_BLOCK {
            found_decoupling = true;
            println!("Found DECOUPLING_BLOCK!");
            break;
        }
    }

    assert!(found_decoupling, "DECOUPLING_BLOCK not found");
}

#[test]
fn test_power_domain_with_decoupling_near_keyword() {
    let code = r#"
board TestBoard {
    power_domain @VCC_5V = 5V @ 1A {
        sources {
            usb: USB_Connector().VBUS;
        }

        decoupling {
            near usb: 10µF @ 1;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify NEAR keyword exists
    let mut found_near = false;
    for token in syntax.descendants_with_tokens() {
        if let rowan::NodeOrToken::Token(t) = token {
            if t.kind() == bhdl_parser::SyntaxKind::NEAR_KW {
                found_near = true;
                println!("Found NEAR keyword");
                break;
            }
        }
    }

    assert!(found_near, "NEAR keyword not found");
}

#[test]
fn test_power_domain_with_decoupling_distributed() {
    let code = r#"
board TestBoard {
    power_domain @VCC_ANALOG = 3.3V @ 500mA {
        sources {
            ldo: LDO_3V3().OUT;
        }

        decoupling {
            distributed: 100nF @ 5;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify DISTRIBUTED keyword exists
    let mut found_distributed = false;
    for token in syntax.descendants_with_tokens() {
        if let rowan::NodeOrToken::Token(t) = token {
            if t.kind() == bhdl_parser::SyntaxKind::DISTRIBUTED_KW {
                found_distributed = true;
                println!("Found DISTRIBUTED keyword");
                break;
            }
        }
    }

    assert!(found_distributed, "DISTRIBUTED keyword not found");
}

#[test]
fn test_power_domain_with_decoupling_each() {
    let code = r#"
board TestBoard {
    power_domain @VCC_IO = 1.8V @ 1A {
        sources {
            buck: TPS54302().OUT;
        }

        decoupling {
            near each io_banks[0..3]: 10µF @ 1, 100nF @ 2;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify EACH keyword exists
    let mut found_each = false;
    for token in syntax.descendants_with_tokens() {
        if let rowan::NodeOrToken::Token(t) = token {
            if t.kind() == bhdl_parser::SyntaxKind::EACH_KW {
                found_each = true;
                println!("Found EACH keyword");
                break;
            }
        }
    }

    assert!(found_each, "EACH keyword not found");
}

#[test]
fn test_power_domain_with_constraints() {
    let code = r#"
board TestBoard {
    power_domain @VCC_3V3 = 3.3V @ 2A {
        sources {
            reg: LM7805().OUT;
        }

        constraints {
            max_ripple: 50mV;
            dropout: 1.5V;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify CONSTRAINTS keyword exists
    let mut found_constraints = false;
    for token in syntax.descendants_with_tokens() {
        if let rowan::NodeOrToken::Token(t) = token {
            if t.kind() == bhdl_parser::SyntaxKind::CONSTRAINTS_KW {
                found_constraints = true;
                println!("Found CONSTRAINTS keyword");
                break;
            }
        }
    }

    assert!(found_constraints, "CONSTRAINTS keyword not found");
}

#[test]
fn test_complete_power_domain_example() {
    let code = r#"
board CompleteBoard {
    // Power domain with all features
    power_domain @VCC_3V3 = 3.3V @ 3A {
        sources {
            main_reg: LM7805().OUT;
            backup_reg: LP5907().OUT;
        }

        distribution {
            mcu.VDD;
            fpga.VCCO[0..3];
            sensors[*].VCC;
            display.VDDIO;
        }

        decoupling {
            near main_reg: 10µF @ 2, 100nF @ 4;
            near each fpga.VCCO[0..3]: 100nF @ 1;
            distributed: 1µF @ 8, 100nF @ 16;
        }

        constraints {
            max_ripple: 50mV;
            dropout: 1.5V;
            startup_time: 10ms;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify all major blocks exist
    let mut blocks_found = vec![false; 4]; // power_domain, sources, distribution, decoupling, constraints

    for child in syntax.descendants() {
        match child.kind() {
            bhdl_parser::SyntaxKind::POWER_DOMAIN_DEF => {
                blocks_found[0] = true;
                println!("✓ Found POWER_DOMAIN_DEF");
            }
            bhdl_parser::SyntaxKind::SOURCES_BLOCK => {
                blocks_found[1] = true;
                println!("✓ Found SOURCES_BLOCK");
            }
            bhdl_parser::SyntaxKind::DISTRIBUTION_BLOCK => {
                blocks_found[2] = true;
                println!("✓ Found DISTRIBUTION_BLOCK");
            }
            bhdl_parser::SyntaxKind::DECOUPLING_BLOCK => {
                blocks_found[3] = true;
                println!("✓ Found DECOUPLING_BLOCK");
            }
            _ => {}
        }
    }

    assert!(blocks_found[0], "POWER_DOMAIN_DEF not found");
    assert!(blocks_found[1], "SOURCES_BLOCK not found");
    assert!(blocks_found[2], "DISTRIBUTION_BLOCK not found");
    assert!(blocks_found[3], "DECOUPLING_BLOCK not found");

    println!("✅ All power domain blocks parsed successfully!");
}

#[test]
fn test_multiple_power_domains() {
    let code = r#"
board MultiDomainBoard {
    power_domain @VCC_5V = 5V @ 2A {
        sources {
            usb: USB_Connector().VBUS;
        }
        distribution {
            motor_driver.VIN;
        }
    }

    power_domain @VCC_3V3 = 3.3V @ 1A {
        sources {
            reg_3v3: LM7805().OUT;
        }
        distribution {
            mcu.VDD;
        }
    }

    power_domain @VCC_1V8 = 1.8V @ 500mA {
        sources {
            reg_1v8: TPS54302().OUT;
        }
        distribution {
            fpga.VCCINT;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Count power domains
    let power_domain_count = syntax.descendants()
        .filter(|node| node.kind() == bhdl_parser::SyntaxKind::POWER_DOMAIN_DEF)
        .count();

    assert_eq!(power_domain_count, 3, "Expected 3 power domains, found {}", power_domain_count);
    println!("✅ Successfully parsed {} power domains", power_domain_count);
}

#[test]
fn test_power_domain_with_complex_capacitor_specs() {
    let code = r#"
board TestBoard {
    power_domain @VCC_CORE = 1.2V @ 10A {
        sources {
            pmic: TPS65950().VCORE;
        }

        decoupling {
            near pmic: 47µF @ 1, 22µF @ 2, 10µF @ 4, 1µF @ 8, 100nF @ 16;
            distributed: 1µF @ 20, 100nF @ 40;
        }
    }
}
"#;

    let parsed = bhdl_parser::parse(code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();

    // Verify multiple CAP_SPEC nodes
    let cap_spec_count = syntax.descendants()
        .filter(|node| node.kind() == bhdl_parser::SyntaxKind::CAP_SPEC)
        .count();

    assert!(cap_spec_count >= 5, "Expected at least 5 capacitor specs, found {}", cap_spec_count);
    println!("✅ Found {} capacitor specifications", cap_spec_count);
}
