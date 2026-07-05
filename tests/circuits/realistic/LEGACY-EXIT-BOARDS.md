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

## Toolchain gap (discovered by fpga_dev_board_comprehensive) — FIXED

Indexed bus-pin refs (`fpga.VCCO[0]`) now parse in v2 arrow statements
(PIN_REF carries a BUS_SUFFIX), literal bus-pin declarations
(`pin VCCO[4]` / `pin D[7:0]`) expand to indexed pin instances in the
netlist, and a suffix-less ref (`fpga.VCCO`) ties the whole bank to a
net as a unit. fpga_dev_board_comprehensive wires VCCO per-index.
Still open: the legacy port-mapping block form (`inst: Type() { PIN <-
net; }`) parses but does not lower connections — v2 arrow statements
are the supported form.
