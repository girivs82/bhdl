# Layout (place & route) — status and handoff

Written 2026-08-18 at `c3dfc01`, when layout work was parked to switch
focus. This is the one document to read before resuming: what ships,
how it is judged, what was tried and parked (with recipes), and the
open tails in priority order. The detailed per-arc ledger lives in the
memory file `reference_kicad_demos_corpus.md`; this is the compressed,
in-repo version.

## 1. State of the tree

- `main` = `c3dfc01`. Working tree clean.
- Every standing gate is green at HEAD:
  - `cargo test --workspace --release`: 112 result lines / 0 FAILED.
  - Corpus sweep `./scripts/sweep_layout_drc.sh` (94 boards, KiCad
    `kicad-cli pcb drc --severity-all`): **0 violations / 0
    unconnected / 0 failed**, 0 LAYOUT-FAILED.
  - Mixer (`tests/circuits/realistic/multichannel_mixer.bhdl`) with
    demo-true jack geometry: seeds 42 / 7 / 99 / 43 free + rigid
    (`BHDL_PNR_RIGID=1`) all **0v / 0unc**; seed 13 = 0v/1unc.
  - ecc83 (`ecc83_srpp.bhdl`, `BHDL_PNR_POUR_GND=1`, seed 42) is
    **byte-identical** to the user-approved golden now checked in at
    `tests/golden/ecc83_srpp.pour_gnd.seed42.kicad_pcb`; run
    `./scripts/guard_ecc83.sh` (exit 0 = identical).

## 2. Doctrine (do not relearn)

- KiCad is the **test oracle only**, never a pipeline dependency.
  Electrical correctness is the bar; "looks like the demo" is not.
- Generic engine judgment only. Fixtures may carry settings / hints /
  intents / pinned placements; the engine may not special-case a board.
- The SHIPPED artifact is the only referee (per-trial logs and dumps
  inside the pipeline tail are per-trial, not the winner).
- A lever that improves its target while regressing a sibling
  configuration is a **trade, not a fix**; dominance exists to refuse
  trades. Never revert real geometry to green a gate.
- ecc83's output must stay byte-identical unless the user approves.
- Measure with the oracle, not the log. Reproduce every quoted number.
- Debug by tracing copper, not by theorising: `BHDL_PNR_TRACE_NEAR`.

## 3. How a board is judged (the currency)

Trials (`run_tiers_speculative`, tiers `[base, si, fanout, cheap-amp,
escape]`, 3 trials each, seed+k) are ranked by `trial_dominated`
(`bhdl-pnr/src/lib.rs`, ~l.86) in this order:

1. residual pad overlaps (legality)
2. `connected_sinks` — router-level, **pre-tail** track-touch proxy
   (`pathfinder::count_connected_sinks`). Not oracle-calibrated
   (`plane_bonus` overcounts pour-net pins); do NOT re-take it on
   shipped copper (tried: made things worse).
3. `pour_defects` (`pour_defect_count`) — DSU witness model incl.
   pour-net dangling ends. Relative currency, not an oracle count
   (a 0-unc board can score >0). Do not gate on it.
4. shipped `drc_violations` (`legalization::check_drc`, re-taken on
   final copper): pad-copper spacing, cross-net copper conflicts,
   signal-net dangling ends, **hole-to-hole (c3dfc01)**, unrouted.
5. measured noise (P5 boards), detour p90/p50 (fidelity boards), HPWL.

Known currency blind spots (oracle sees, we do not): zone-zone
clearance between two pours; starved thermals in some placements;
pour-net dangling as a *verdict* (priced only in 3).

Sign-off (`bhdl-cli layout`, 277d6d5): exit 1 when shipped DRC > 0 or
geometry verify FAILs; artifacts always written; `BHDL_SIGNOFF_ADVISORY=1`
demotes to a warning. `--fab-profile standard|fine|coarse` adds fab
preflight (887f4fc) to the gate; preflight always *reports*.

## 4. What shipped in the last stretch (newest first)

| commit | what |
|---|---|
| c3dfc01 | hole-to-hole class in `check_drc` (via↔via, via↔THT; oracle rule (d1+d2)/2+0.25). |
| 4f740f1 | demo-true Neutrik NMJ6HCD2 jacks (generator was mirrored in X; every jack's switch-normal column was outboard) + three tail bugs: unserved-pad rung silently dropped pads whose nearest same-net copper was on the other layer; split-group join offered B.Cu-only endpoints as F.Cu starts and graded a route ending on a fill vertex as stray; unserved ML rung targeted only the far main. New tracer `BHDL_PNR_TRACE_NEAR=x,y,r`. |
| 887f4fc | fab preflight (`output/preflight.rs`): trace/space/drill/annular/hole-to-hole (slots as capsules)/copper-to-edge vs a profile; first real catch was the jack chirality. |
| 277d6d5 | layout exit status = sign-off verdict. |
| 42d48e4..a2fa628 | pour-net dangling priced; signal-net dangling in DRC; shipped DRC in currency; seam close; pad-copper DRC; split-group join = true-geometry convergence. |

## 5. Parked levers — measured, reverted, patches preserved

Patches live in `bhdl-pnr/docs/parked/` and apply cleanly on `c3dfc01`
(`git apply bhdl-pnr/docs/parked/<name>.patch`).

### 5.1 `anchored_blocks.patch` — placement seed for grouped channels
- Finding: the mixer channels ARE functional groups (expansion
  parents), but `layout_block_netlist_driven` stacks every child in one
  row → a 25-member block is 221 mm wide on a 110 mm board and the
  shelf packer scatters it; that is why an op-amp can straggle 30 mm
  from its pot column.
- Lever: seed a block that has position-FIXED members at their centroid
  (skip the shelf) + wrap side stacks into lanes past max(2×span, 30mm).
- Effect: blocks 33×62, channels compact, ecc83 byte-identical, seed 13
  7unc→0. BUT seed entropy collapsed (42/7/43 shipped byte-identical
  boards) and that winner carries a zone-zone clearance + GND stub the
  currency can't price → 7/43 2v, 99 1v, uno 1unc. **Trade → parked.**
- Resume: give anchored blocks per-seed jitter/rotation of the internal
  layout; price zone-zone clearance (5.3) first; then re-measure
  42/7/99/43/13/rigid + corpus.

### 5.2 `via_guard.patch` — path-local via spacing in the maze
- Finding: the main router emits `F → via → one B cell → via → F`, two
  drills 0.30 mm apart (hole_to_hole). Each repair rung drill-checks its
  own additions but the maze has no path-local rule.
- Lever: in `pathfinder.rs` via expansion, walk the settled predecessor
  chain back within `drill+0.25` and refuse a via if that stretch already
  contains a layer change.
- Effect: correct (seed 42 clean, HPWL 2333→2313) but it reroutes
  wherever it fires and the tails land differently: 7/43 → 1v
  starved_thermal, 99 → 1unc. **Parked** until the tails it moves are
  priced (starved thermal is not in the currency).

### 5.3 Zone-zone clearance (vbias split pour vs GND) — diagnosed, unfixed
- On the anchored-blocks board the two fills come within 0.23–0.28 mm
  at (66.1, 91.5): a vbias fill *tongue* (not a pad blob) 0.23 mm from a
  GND *fragment* edge along a GND track at y=91.05.
- The GND fill's foreign claim = a copper-grid simulation of the vbias
  fill (`plane_foreign_holes_on` deflated by (r−0.1875)/1.082, no
  spokes/fixpoint), dilated zc+0.2, in both the writer
  (`kicad.rs` ~l.338) and the emission model (~l.2663).
- Tried: claim from `emission_fill_polys(vbias)` rasterised. **7×
  runtime** (3065 s vs ~420 s: a vbias fill per GND emission) AND the
  clash was still there → the writer's vbias zone ≠ the model at that
  tongue, or that GND fragment isn't under the claim mask. Reverted.
- Resume: apply 5.1, run seed 42 with `BHDL_PNR_DUMP_FINAL_MIRROR=dir`,
  diff the writer's vbias polygon vs the model's at (65–67, 91–93);
  find the divergence before choosing a lever. If the emission route is
  ever taken, cache the claim per routes-hash.

### 5.4 Span-only amputation (validator) — CLOSED after 6 landings
- Root-cause-correct for the RK09K pot posts, but every landing shipped
  debris on a *different* net that no local cleanup can reach; the
  structural reason is that pre-tail `connected_sinks` outranks shipped
  DRC. Do not build a 7th variant; the fix is an oracle-calibrated
  shipped-connectivity currency (DSU on final copper incl. signal nets).
- Pot posts recipe (not applied): in `ipc7351.rs`
  `generate_pot_rk09k_v`, add
  `make_slot_pad("MP", 0.0, -4.4, 3.0, 4.0, 1.8, 2.1, PadShape::RoundedRectangle)`
  and the +4.4 twin, `pad_count: 5`.

## 6. Open tails, in the order I'd take them

1. **Zone-zone clearance** — 5.3, cheap decisive diagnosis first.
2. **Currency: shipped connectivity** the oracle agrees with (DSU on
   final copper, all nets) so it can outrank the pre-tail sink proxy.
   Unblocks 5.4 and makes 5.1/5.2 measurable honestly.
3. **Anchored blocks** (5.1) once 1–2 exist; then **via guard** (5.2).
4. Preflight: the router default 0.150 mm trace/space is 2 µm under the
   6-mil "standard" floor (93/94 boards flag it). User knob today
   (`clearance 0.16`); a router-default change moves every board — decide
   deliberately.
5. Seed 13 (0v/1unc), per-pad rotation model, fab-house preflight
   profiles from real capability sheets, seed entropy (only block-order
   rotation today — 42/7 collide modulo block count).

## 7. Tooling knobs worth remembering

- Run: `env BHDL_LIB_PATH=$PWD BHDL_JLCPARTS_DB=/nonexistent ./target/release/bhdl-cli <fixture> layout -o out.kicad_pcb --seed N`
- Oracle: `/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli pcb drc --format json --severity-all -o out.json out.kicad_pcb`
- Corpus gate: `./scripts/sweep_layout_drc.sh` (~15 min; logs in `/tmp/bhdl_layout_drc/`).
- ecc83 byte-guard: `./scripts/guard_ecc83.sh`.
- `BHDL_PNR_TRACE_NEAR=x,y,r` — every segment/via of every net near a
  point after each tail phase (after-unserved / after-split-join /
  after-via-prune / after-seam-close). Use with
  `BHDL_PNR_NO_SPECULATION=1 --trials 1` for a single deterministic tail.
- `BHDL_PNR_PROBE_AT=x,y` — unserved-pad rung verdict for one pad.
- `BHDL_PNR_DUMP_FINAL_MIRROR=dir` — winner-only emission-model fill dump.
- `BHDL_PNR_RIGID=1`, `BHDL_PNR_POUR_GND=1`, `BHDL_PNR_TIMING=1`,
  `BHDL_PNR_MAX_TRIAL_THREADS`, `BHDL_PNR_SERIAL_TRIALS=1`.
- Mixer run ≈ 6–8 min; never run two mixers concurrently for timing
  claims (contention faked an 11× regression once). Rebuild after any
  checkout before measuring; the binary is replaced under a running
  batch, so start batches only after `cargo build` finishes.
- The trace/verify/preflight reports print after "PnR Results"; the
  sign-off line is the last thing printed.
