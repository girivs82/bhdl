# Schematic Rendering V4 — Idiom Composition, Not Graph Layout

> **Status:** architecture spec. V4.1 (this document + backbone classifier)
> in progress. Three prior attempts failed; §1 records why, so attempt four
> cannot un-learn it.

## 1. Why three attempts failed

Attempts 1–3 treated the schematic as a generic graph-layout problem over
the netlist, rendered by a Canvas JS engine (attempt 3:
`bhdl-schematic/viewer/schematic.js`, ~4.7k lines, layout and rendering
interleaved). Observed on the flagship buck (a 9-component circuit any EE
sketches in 30 seconds):

- Components in ONE undifferentiated row at uniform pitch — a list, not a
  circuit. The V_IN → IC → L → V_OUT power flow is invisible.
- The IC renders as an empty box with NO pin stubs, so its defining
  connections (VIN/SW/FB/BOOT) cannot be drawn at all.
- Almost no wires: connectivity implied by floating dots; the two rails
  the caps decouple are visually indistinguishable; port flags float
  disconnected.
- Label collisions.

The root error is architectural: **a good schematic is not a laid-out
graph — it is a composition of electrical idioms** (a buck stage, a
feedback divider, a decoupling bank, a rail bus). Generic layout cannot
recover idioms from an undirected graph; BHDL never has to, because its
semantic model still KNOWS them.

## 2. What BHDL knows that generic layout cannot

| Semantic fact | Drawing decision it makes |
|---|---|
| `pin X: power in` / `ground` / `signal out` | IC symbol sides: inputs left, outputs right, GND bottom, aux top; ground symbols point down |
| `power VCC = 5V @ 1A` rails + supply tree | Horizontal rail buses top; stage order left→right = power-up order |
| `expansion_parent` groups | A regulator's application circuit is a PRE-COMPOSED block (the datasheet figure), placed as a unit |
| `component_class` (capacitor/resistor/…) | Symbol choice; decoupling caps cluster at their IC |
| Net classes (Power/Ground/Signal) | Ground drops down; rails bus horizontally; signals flow left→right |
| Pin directions on signal nets | Column (topological) ordering of the signal path |

## 3. Architecture

All LAYOUT moves to Rust (`bhdl-schematic/src/v4/`), emitting positioned
geometry and SVG. The HTML shell keeps only pan/zoom/hover. Rendering is
therefore deterministic, unit-testable, and embeddable in the synthesis
report.

Pipeline per sheet:

1. **Classify** (`v4::classify`) — partition the netlist:
   - rails (Power-class nets) and ground;
   - **stages**: connected regions between rails (a supply's expansion
     group, or the region between rail A and rail B);
   - within a stage: the **series backbone** — the chain of elements
     walked from the source rail to the target rail through two-terminal
     parts and through ICs (enter by power-in/signal-in, exit by
     power-out/signal-out);
   - **shunts** — parts with one pin on a backbone net and one on ground
     (decoupling/output banks), attached to their backbone net;
   - **loops** — chains that leave the backbone and re-enter it upstream
     (feedback dividers), rendered as orthogonal return paths;
   - **residue** — anything unclassified (honest fallback, §5).
2. **Compose** (`v4::compose`) — lay out each stage on a grid: backbone
   left→right on the spine row; shunts drop below their tap point in
   net order with ground symbols; loop returns route above/below the
   spine; rails render as labeled bus segments feeding stage inputs;
   stages tile left→right in supply-tree order, one row per tree branch.
3. **Render** (`v4::svg`) — symbol library (IEC-style R/C/L/diode/LED,
   polarized-cap plates, opamp triangle, IC box WITH pin stubs + names),
   orthogonal wires with junction dots, net flags for long-range hops
   (never a rat's nest: a connection farther than K grid units becomes a
   named flag pair — standard EE practice).

## 4. What "good" means (acceptance)

The flagship buck must read like the TI datasheet application figure:
input caps hanging off a VIN bus into the IC's left side, SW leaving the
right side through the inductor to a VOUT bus with the output bank
hanging below it, the FB divider tapped off VOUT returning to FB, BOOT
cap arcing to SW, every wire drawn, no text collisions. Acceptance is a
screenshot review against that description, per corpus board class.

## 5. Real-Data discipline applied to drawing

The classifier never guesses: a part it cannot idiomize goes to the
residue region, drawn in a plain grid WITH its wires as named net flags —
ugly but truthful, and listed in the render's own absence ledger
("N components unidiomized") so sheet quality is measurable corpus-wide,
exactly like ERC024. The corpus sweep gains a `visualize` column:
PASS = zero unidiomized, zero label collisions (both machine-checkable).

## 6. Phasing

- **V4.1** — this spec; `v4::classify` with unit test on the flagship
  (backbone = [c_in bank] VIN-rail → reg → l_out → VOUT-rail [c_out
  bank, FB loop]).
- **V4.2** — compose + svg for the power-stage idiom; flagship screenshot
  acceptance.
- **V4.3** — signal-path idiom (opamp chain: precision_opamp fixture),
  IC-centric boards (MCU with peripheral fan-out uses net flags).
- **V4.4** — corpus sweep + absence-ledger gate; retire the JS layout.
