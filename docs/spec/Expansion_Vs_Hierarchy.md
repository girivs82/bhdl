# Expansion vs. Hierarchical Modules — a design rationale

Status: reference (settled 2026-08-21). This document records why BHDL
models application circuits with **virtual pins + expansion blocks**
rather than classical sealed hierarchical modules, and exactly where
the two mechanisms are equivalent and where they are not. The question
it answers: *"is the virtual-pin mechanism not the same as an entity
with three ports?"*

## The short answer

At the **interface level, yes** — they are the same thing. An entity
exposing `VIN / VOUT(virtual) / GND` presents exactly the port surface
a three-port module would. That equivalence is deliberate and
load-bearing: it is why the power tree's swap-by-rename works. A
`GenericBuck` placeholder is a three-port module with empty internals;
a real `TPS54331` is a three-port module with internals. The regulator
pin contract (`VIN / VOUT virtual / GND`, machine-enforced by
`stdlib_regulators_honor_the_pin_contract`) is precisely this shared
port algebra.

The difference is **not provenance**. It is **who owns the children,
and whether the namespace is sealed or open**.

## The two interior semantics

A classical hierarchical module **seals** its internals. The board
sees ports; the contents are private; reaching inside requires
hierarchy machinery — hierarchical refdes, BOM flattening,
hierarchical placement, hierarchical fault traversal. This is the
Verilog/VHDL model, and those languages then spent decades adding
`generate`, `defparam`, and hierarchical references to poke holes back
through the seal.

BHDL's expansion does the opposite: the entity **contributes** parts
into the board's flat namespace, and **the board keeps authority over
them**. `U1`'s expansion mints `U1_C_out` as a first-class board part
on board nets.

## Where the difference is load-bearing

Every one of these depends on expansion children living in the board's
flat, open namespace. Each would require new (and unwanted) hierarchy
machinery under a sealed-module design:

1. **Board takeover** (ERC029 doctrine). When the board hand-authors
   the application circuit — its own output filter on the delivery
   path while the virtual pin is unwired — the expansion *yields* and
   skips exactly those children. A sealed module cannot yield: its
   `C_out` exists whether or not the board provided a better one, and
   the duplicate floats.

2. **Conditional materialization** (live-children gating). Children
   mint only when the board wires the parent pins that make them
   meaningful: the ATmega's I²C pullups appear only if the board uses
   I²C on PC4/PC5; a board using those pins as ADC inputs must not be
   loaded by them. A static module always contains everything, and a
   parameterized one pushes the decision to the instantiation site —
   the wrong side, since the *wiring* already states the answer.

3. **Children are first-class board parts.** An expansion child gets a
   board refdes (`U1_C_out` → `C5` via the one refdes allocator), its
   own BOM line, its own placement, its own DNP in a board SKU
   variant, and — critical to the safety machine — it is an
   **individually enumerable fault**: the whole-universe FMEDA
   campaign shorts/opens each expansion cap as a separate λ
   contribution, and decap-margin verification opens synthesized caps
   one at a time. All of this is flat-part enumeration.

4. **Flatness matches the artifact.** A fabricated board *is* flat:
   fab data, BOM, pick-and-place, rework all speak flat refdes.
   Schematic hierarchy is presentation, not structure. PnR, ERC, the
   SPICE converter, freeze, and the elaboration round-trip gate all
   operate on the flat netlist; sealed submodules would force a
   flatten/unflatten pair around every one of them.

5. **Cross-boundary value derivation.** `design { }` blocks compute
   child values from board context — the LM317 divider from the
   instantiation's `v_out`, tube bias networks from board intents.
   This is parameterizable in a module system too, but it only
   composes cleanly with takeover/gating when the children share the
   board's namespace: a computed child the board then overrides is one
   rule, not two mechanisms fighting.

## Where they are equivalent

- **Port surface / substitutability.** Interface-compatible entities
  interchange by rename. This is module-like on purpose, and it is the
  entire basis of the power tree's placeholder→real-part flow.
- **Encapsulation of knowledge.** Both put the application circuit
  next to the part. Expansion adds the discipline that the recipe is
  *datasheet-sourced* (Real-Data Policy) — but that is a library
  convention layered on the mechanism, not the mechanism itself.

## Why the question doesn't arise at design time

Expansion was built to answer *"how does a datasheet's mandatory
application circuit travel with the part?"* — a provenance-and-
correctness question about vendor knowledge. The port-surface
equivalence only becomes visible when **substitutability** is needed —
which happened when the power tree required generic placeholders that
later swap to real parts. The two mechanisms answer different
questions and merely share an interface; nothing at expansion-design
time forced the comparison.

## Open ledger item: designer-owned composition

BHDL currently has no *designer-owned* composition mechanism. A
designer wanting a reusable three-port block of their own — "my 60A
phase": controller + FETs + inductor + sense network — would today
write an entity with an expansion block. That works, but it borrows
machinery whose discipline (datasheet-sourced values, vendor
provenance) was designed for library parts.

If a real board demands it, the shape is a `block` (name open) that
**reuses the expansion engine** — same flat contribution, same
takeover and gating rules — with *designer* provenance labeling
instead of datasheet provenance. It should not be built speculatively;
this paragraph exists so the eventual need lands on a recorded
decision instead of a re-derivation.
