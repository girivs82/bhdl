# Legacy EXIT boards — triage record (2026-07-05)

The 2026-07-05 regeneration pass restored 24 boards to the working corpus
(sweep baseline 27 → 51 passing, zero regressions): 20 by import injection +
legacy pin renames, 4 by targeted v2 rewrites. The boards below still EXIT;
every one depends on a RETIRED v1 language feature. Per the regenerate-not-
patch policy they are candidates for rewrite-without-the-feature or
retirement — each needs a per-board judgment on whether it still tests
anything the current language has.

| Board | Retired feature (first parse error) |
|---|---|
| 555_astable_oscillator.bhdl | Unexpected token in board definition    Expected SEMI, found Some(L_BRACE) |
| 7805_with_intents.bhdl | Expected SEMI, found Some(FOR_KW)    Unexpected token in board definition |
| buck_converter_metadata.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(TYPE_KW)    Expecte |
| buck_converter_stable.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(TYPE_KW)    Expecte |
| buck_converter_topology_aware.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(TYPE_KW)    Expecte |
| buck_converter_tps54302.bhdl | Expected SEMI, found Some(LEFT_ARROW)    Unexpected token in board definition |
| buck_converter_with_intents.bhdl | Unexpected token in board definition    Expected SEMI, found Some(L_BRACE) |
| cascaded_bucks.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(TYPE_KW)    Expecte |
| ddr3_routing_example.bhdl | Expected ',' or ')' in parameter list    Expected R_PAREN, found Some(ERROR_TOKEN) |
| fpga_dev_board_comprehensive.bhdl | Expected L_BRACE, found Some(DOT_DOT)    Unexpected token in block |
| intent_system_demo.bhdl | Expected SEMI, found Some(FOR_KW)    Unexpected token in board definition |
| mixed_signal_with_intents.bhdl | Unexpected token in board definition    Expected SEMI, found Some(L_BRACE) |
| multi_voltage_comprehensive.bhdl | Expected AT, found Some(SEMI)    Expected literal, identifier, or '(' for expression facto |
| power_supply_constraints.bhdl | Expected SEMI, found Some(L_BRACE)    Unexpected token in board definition |
| precision_opamp_circuit.bhdl | Expected NUMBER, found Some(MINUS)    Expected SEMI, found Some(MINUS) |
| test_7805_regulator.bhdl | Expected SEMI, found Some(COLON)    Unexpected token in board definition |
| test_7805_regulator_v1.bhdl | Expected SEMI, found Some(L_BRACE)    Unexpected token in board definition |
| test_7805_simple.bhdl | Expected SEMI, found Some(L_BRACE)    Unexpected token in board definition |
| test_7805_with_types.bhdl | Expected IDENT, found Some(POWER_KW)    Expected EQ, found Some(POWER_KW) |
| test_array_pin_access.bhdl | Expected L_BRACE, found Some(DOT_DOT)    Unexpected token in block |
| test_generate_wildcard.bhdl | Expected L_BRACE, found Some(DOT_DOT)    Unexpected token in block |
| test_hierarchical_wildcard.bhdl | Expected COLON, found Some(L_BRACKET)    Expected literal, identifier, or '(' for expressi |
| test_minimal_parse.bhdl | Expected SEMI, found Some(L_BRACE)    Unexpected token in board definition |
| test_simple_flow.bhdl | Expected a top-level item (e.g., 'board', 'entity', 'interface', 'testbench', etc.), found |
| test_v2_complete.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(DISTRIBUTION_KW)    |
| test_v2_complete_fixed.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(DISTRIBUTION_KW)    |
| test_v2_comprehensive_eda.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(DISTRIBUTION_KW)    |
| test_v2_generate.bhdl | Expected L_BRACE, found Some(DOT_DOT)    Unexpected token in block |
| test_v2_generate_named.bhdl | Expected L_BRACE, found Some(DOT_DOT)    Unexpected token in block |
| test_v2_interface.bhdl | interface-as-component (`x: I2C(…)`) |
