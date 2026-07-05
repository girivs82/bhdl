# Legacy EXIT boards — campaign CLOSED (2026-07-05)

Every board in the corpus now passes. Across the three passes: 27 → 51 →
57 → 60 passing, zero regressions at any step.

- 24 boards regenerated mechanically (imports + legacy pin renames).
- 9 real circuits re-authored to v2 (555 astable, precision op-amp ±12V
  chain, buck_converter_tps54302/with_intents/stable, mixed-signal +
  intent demos, multi-voltage showcase, FPGA dev board). The final three
  are pure supply-statement designs — and the live ERC shaped them:
  ERC017 vetoed LM317 for the 5V→1.8V rail (2V dropout), ERC004 forced
  the MCU onto 3.3V (5V TX into 3.3V/1.8V parts needs a level shifter).
- 21 boards retired: dead-dialect parse stubs, retired-feature smoke
  tests, and coverage duplicates (each deletion recorded in the git log
  of commits 9f6d346 and the campaign-closing commit).

## Known toolchain gap (discovered by fpga_dev_board_comprehensive)

Bus-pin DECLARATIONS parse (`pin VCCO[4]: power in;`) but indexed refs
(`fpga.VCCO[0]`) do NOT parse in v2 arrow statements (the PIN_REF path in
bhdl-parser/src/expressions.rs has no bracket handling), and the legacy
port-mapping block parses but silently fails to lower connections. Until
fixed, bus pins are wired as a unit.
