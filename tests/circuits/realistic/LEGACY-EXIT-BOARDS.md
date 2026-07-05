# Legacy EXIT boards — triage record (updated 2026-07-05, rewrite-or-retire pass)

Second pass complete: 16 boards RETIRED (dead-dialect parse stubs and 7805/
coverage duplicates — deleted), 6 real circuits REWRITTEN to v2 and passing
(555 astable, buck_converter_tps54302, buck_converter_with_intents,
mixed_signal_with_intents, intent_system_demo, precision_opamp_circuit —
the last also drove the negative-rail grammar fix `power VEE = -12V`).
The 8 boards below remain EXIT: all are DEEP v1 dialect (implicit
instances from refdes prefixes, inline `Module` aliases, power_domain
distribution DSL, DOT_DOT ranges) whose rewrite means re-authoring the
whole board. cascaded_bucks' scenario is meanwhile covered by
test_supply_tree.bhdl (S4b).

| Board | Retired feature (first parse error) |
|---|---|
| buck_converter_metadata.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(TYPE_KW)    Expecte |
| buck_converter_stable.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(TYPE_KW)    Expecte |
| buck_converter_topology_aware.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(TYPE_KW)    Expecte |
| cascaded_bucks.bhdl | Expected literal, identifier, or '(' for expression factor, found Some(TYPE_KW)    Expecte |
| ddr3_routing_example.bhdl | Expected ',' or ')' in parameter list    Expected R_PAREN, found Some(ERROR_TOKEN) |
| fpga_dev_board_comprehensive.bhdl | Expected L_BRACE, found Some(DOT_DOT)    Unexpected token in block |
| multi_voltage_comprehensive.bhdl | Expected AT, found Some(SEMI)    Expected literal, identifier, or '(' for expression facto |
| power_supply_constraints.bhdl | Expected SEMI, found Some(L_BRACE)    Unexpected token in board definition |
