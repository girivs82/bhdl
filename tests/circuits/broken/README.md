# Broken / Aspirational Circuits

⚠️ Circuits in this directory **do not parse or build** with the current BHDL
grammar. They are kept for design-intent reference only. Do **not** treat them
as working fixtures, and do not point the test harness at this directory.

When a circuit here is made to parse + synthesize against the supported syntax,
move it back into the appropriate `tests/circuits/{simple,realistic,...}/`
directory.

## `buck_regulator_12v_to_5v.bhdl`

12V→5V buck regulator (TPS54360). Aspirational fixture using several constructs
the grammar does not accept:

- A nested `entity TPS54360 { ... }` defined *inside* the `board { ... }` block
  (entities must be top-level).
- `@` inside a constructor arg, e.g. `Ferrite(600ohm@100MHz)`.
- Positional bare-word/identifier constructor args, e.g.
  `Cap(220uF, 25V, electrolytic)`, `Fuse(5A, fast)`, `TVSDiode(SMAJ15A, 15V)`,
  `Res(10k, 0603, 1%)`, `LED(green, 0603)`. Supported circuits use keyword args
  (`Cap(220µF, voltage=25V)`) instead.
- A `testpoint TP_X: net;` keyword (supported circuits instantiate a
  `TestPoint()` entity instead).
- Inline `SeriesRC: Series { ... }` composition blocks.

See `tests/circuits/realistic/buck_converter_tps54331.bhdl` for an equivalent
buck converter expressed in supported v2.0 syntax.
