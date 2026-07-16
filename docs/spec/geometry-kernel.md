# P1 Geometry Kernel — clearance-by-construction routing

## Thesis

The pipeline today is grid-route → exact-validate → amputate →
recover. Every failure family of the last milestones traces to the
seam between the 0.3mm cell model and exact copper geometry:
quantized escapes, partial-cell ring claims, extension copper the
validator re-judges, and the amputate/rebuild loops that follow.
The kernel inverts the contract: copper is checked against EXACT
geometry at construction time, so the validator's role shrinks to
a backstop instead of a co-author.

The measured ceiling this unlocks: uno 31 / class 9 / cbt6 9
unconnected are fixed-outline 2-layer capacity AT GRID RESOLUTION —
off-grid packing and true push-and-shove need continuous geometry.

## Milestones

- **M1 (this arc): one truth for clearance.** `geom.rs` — exact
  primitives (point/segment/rect/poly distances, the 1µm rule-exact
  epsilon) + `ClearanceIndex`: a bucketed spatial index over a
  board's copper (segments, vias, pads, edge, cutouts) answering
  `first_conflict(candidate_segment) -> Option<Conflict>` exactly.
  The three drifting predicate copies (validator, meander_clear,
  plane-drop stub checks) migrate onto it. Behavior-preserving.
- **M2: off-grid last mile.** A continuous single-net escape/connect
  router (visibility over the ClearanceIndex) used by completion for
  walled-in sinks — attacks the uno tail directly.
- **M3: clearance-by-construction main routing.** Grid guides
  globally (congestion negotiation stays), the kernel legalizes
  each accepted path exactly at commit time — no more late
  amputation of freshly routed copper.
- **M4: push-and-shove.** Deform neighbor polylines under the
  ClearanceIndex instead of rip-and-rebuild; the targeted R&R from
  the tail arc becomes the fallback, not the tool.

## Doctrine carried over

- The KiCad DRC oracle grades everything; where it is blind (saved
  fills vs interior Edge.Cuts) the kernel's own exact check is the
  gate.
- Rule-exact geometry is legal: all comparisons use gap − 1µm.
- Unrouted beats illegal, always.
