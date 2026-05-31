//! Interface-constraint provenance — the typed origin record carried
//! alongside each `intf_const__*` / `intf_const_rel__*` attribute.
//!
//! # Why this exists
//!
//! v0.8 interface `constraints { }` blocks lower to flat module
//! attributes (`intf_const__<pin>__<prop> = <value>`), consumed by the
//! P&R session's boundary reader. Two tier-2 gaps motivated this module
//! (task #96, and the P&R↔synth handshake §10/§11):
//!
//! 1. **Silent overwrite.** When two constraint statements in one
//!    interface target the same `(pin, prop)` — e.g. a wildcard
//!    `*: single_ended 40ohm` followed by a specific
//!    `DQ0: single_ended 50ohm` — the second `insert` clobbered the
//!    first in the attribute `HashMap`, with no record that a more- and
//!    a less-specific rule disagreed. We now keep *all* contributors and
//!    pick a winner by target specificity (see [`ConstraintTier`] and the
//!    synth-side `apply_iface_constraints`).
//!
//! 2. **No origin.** A downstream conflict diagnostic ("net N: 40ohm vs
//!    50ohm") could name the pins but not *where* each rule was written.
//!    This struct carries the source line + the declaring interface type
//!    name + the tier, so P&R can render a traceable message.
//!
//! # Wire format
//!
//! The synth side emits **one** sidecar module attribute,
//! [`INTERFACE_CONSTRAINT_PROVENANCE_ATTR`], whose value is a
//! `serde_json` object mapping each constraint attribute key to its
//! ordered list of contributors:
//!
//! ```text
//! intf_const__<pin>__<prop>   = "50ohm"          # winning value (unchanged, back-compat)
//! intf_const_provenance       = {"intf_const__ddr.lane0.DQ0__single_ended":
//!                                  [{"value":"40ohm","line":34,"tier":"Interface","scope":"DDR4Data"},
//!                                   {"value":"50ohm","line":51,"tier":"Specific","scope":"DDR4Data"}]}
//! ```
//!
//! A single attribute regardless of constraint count (the handshake
//! flagged per-constraint sidecar doubling as a concern). The consumer
//! reads the winner from the primary `intf_const__*` attribute and the
//! full contributor list — the input a same-scope contradiction check
//! needs — from the map. The primary attributes are untouched, so a
//! consumer that ignores the map behaves exactly as before. Decode the
//! map with `serde_json::from_str::<ConstraintProvenanceMap>(...)`.
//!
//! # Relationship to the P&R `ConstraintSource`
//!
//! `bhdl-pnr`'s `ConstraintSource { file, line, intent_kind,
//! recipe_version }` is the lowered-constraint provenance. This struct is
//! the *upstream* record: `line` maps directly; `scope` (the interface
//! type name) + `tier` give the P&R reader what it needs to build a
//! `ConstraintSource` (its `intent_kind` is `interface:<prop>`, already
//! reconstructed from the attribute key). `file` is intentionally absent
//! for now — threading the defining `.bhdl` path through the import
//! loader to the materialiser is a separate, larger change; line +
//! interface name are traceable without it.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Module-attribute key holding the constraint-provenance map for an
/// interface-bearing module. The value is a `serde_json` serialization of
/// a [`ConstraintProvenanceMap`] — one attribute per module regardless of
/// how many constraints it carries.
pub const INTERFACE_CONSTRAINT_PROVENANCE_ATTR: &str = "intf_const_provenance";

/// The provenance sidecar payload: maps a constraint attribute key (e.g.
/// `intf_const__ddr.lane0.DQ0__single_ended` or
/// `intf_const_rel__ddr.CK.P__ddr.CK.N__length_match`) to the ordered
/// list of contributors that targeted it. Serialized to JSON under
/// [`INTERFACE_CONSTRAINT_PROVENANCE_ATTR`].
pub type ConstraintProvenanceMap = HashMap<String, Vec<ConstraintProvenance>>;

/// The declaration tier a constraint came from. Determines override
/// precedence when two contributors target the same `(pin, prop)`:
/// higher tier wins. Within a single interface block, a constraint that
/// names an explicit pin (`DQ0: …`) is [`ConstraintTier::Specific`] and
/// overrides a wildcard (`*: …`) which is [`ConstraintTier::Interface`].
///
/// `Entity` and `Board` are reserved for the future entity-level /
/// board-level constraint-override grammar (not yet parsed); they are
/// defined now so the precedence ladder and the wire format don't change
/// shape when that grammar lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConstraintTier {
    /// Declared via a wildcard target (`*`, `DQ*`, `CK.*`) in an
    /// interface `constraints { }` block — the broad default.
    Interface,
    /// Declared via an explicit, fully-qualified pin target in an
    /// interface `constraints { }` block — overrides a wildcard.
    Specific,
    /// Reserved: declared in an entity-level override block.
    Entity,
    /// Reserved: declared in a board-level override/addition block.
    Board,
}

impl ConstraintTier {
    /// Stable lowercase token used in the wire format.
    pub fn as_str(&self) -> &'static str {
        match self {
            ConstraintTier::Interface => "interface",
            ConstraintTier::Specific => "specific",
            ConstraintTier::Entity => "entity",
            ConstraintTier::Board => "board",
        }
    }

    /// Parse a wire token back to a tier. Unknown tokens map to
    /// [`ConstraintTier::Interface`] (the safe broad default) so an
    /// older consumer reading a newer producer degrades rather than
    /// failing.
    pub fn from_str_lenient(s: &str) -> ConstraintTier {
        match s {
            "specific" => ConstraintTier::Specific,
            "entity" => ConstraintTier::Entity,
            "board" => ConstraintTier::Board,
            _ => ConstraintTier::Interface,
        }
    }
}

/// One contributor to a constraint value: the value itself plus where it
/// came from. A `(pin, prop)` slot may have several of these when more
/// than one statement targets it; the highest-`tier` one wins (ties
/// broken by source order = last writer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintProvenance {
    /// The property value text as written (e.g. `"40ohm"`, `"DATA"`,
    /// `"1ps"`, `"true"`).
    pub value: String,
    /// 1-based source line within the declaring interface's file, if
    /// recoverable from the syntax tree. `None` if unavailable.
    pub line: Option<u32>,
    /// The declaration tier (override precedence).
    pub tier: ConstraintTier,
    /// The declaring interface *type name* (e.g. `"DDR4Data"`) — the
    /// human-locatable scope. Combined with `line` this is enough to
    /// find the statement without the absolute file path.
    pub scope: String,
}

impl ConstraintProvenance {
    pub fn new(
        value: impl Into<String>,
        line: Option<u32>,
        tier: ConstraintTier,
        scope: impl Into<String>,
    ) -> ConstraintProvenance {
        ConstraintProvenance {
            value: value.into(),
            line,
            tier,
            scope: scope.into(),
        }
    }

    /// The winning contributor of a list by override precedence: highest
    /// [`ConstraintTier`], ties broken by source order (last writer
    /// wins). Returns `None` for an empty list.
    pub fn winner(entries: &[ConstraintProvenance]) -> Option<&ConstraintProvenance> {
        entries.iter().reduce(|best, cur| {
            // `cur` wins on a tie (>=) so later same-tier statements
            // override earlier ones — matches HashMap last-writer.
            if cur.tier >= best.tier { cur } else { best }
        })
    }

    /// `true` if the list has at least two contributors with **different
    /// values at the same tier** — a genuine same-scope contradiction
    /// (as opposed to an intentional specific-over-wildcard override,
    /// which differs in tier). This is the synth-visible, within-one-
    /// module signal; cross-net contradictions are P&R's to detect after
    /// net-merge (handshake §10).
    pub fn has_same_tier_conflict(entries: &[ConstraintProvenance]) -> bool {
        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                if entries[i].tier == entries[j].tier
                    && entries[i].value != entries[j].value
                {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_map_roundtrip_and_winner() {
        let mut map: ConstraintProvenanceMap = HashMap::new();
        map.insert(
            "intf_const__ddr.lane0.DQ0__single_ended".to_string(),
            vec![
                ConstraintProvenance::new("40ohm", Some(34), ConstraintTier::Interface, "DDR4Data"),
                ConstraintProvenance::new("50ohm", Some(51), ConstraintTier::Specific, "DDR4Data"),
            ],
        );
        let json = serde_json::to_string(&map).unwrap();
        let back: ConstraintProvenanceMap = serde_json::from_str(&json).unwrap();
        assert_eq!(back, map);
        // Specific overrides Interface.
        let entries = &back["intf_const__ddr.lane0.DQ0__single_ended"];
        assert_eq!(ConstraintProvenance::winner(entries).unwrap().value, "50ohm");
    }

    #[test]
    fn last_writer_wins_on_tie() {
        let entries = vec![
            ConstraintProvenance::new("40ohm", Some(1), ConstraintTier::Interface, "X"),
            ConstraintProvenance::new("60ohm", Some(2), ConstraintTier::Interface, "X"),
        ];
        assert_eq!(ConstraintProvenance::winner(&entries).unwrap().value, "60ohm");
    }

    #[test]
    fn same_tier_conflict_detection() {
        let conflict = vec![
            ConstraintProvenance::new("40ohm", Some(1), ConstraintTier::Interface, "X"),
            ConstraintProvenance::new("60ohm", Some(2), ConstraintTier::Interface, "X"),
        ];
        assert!(ConstraintProvenance::has_same_tier_conflict(&conflict));

        // Specific-over-wildcard is an override, not a conflict.
        let override_ = vec![
            ConstraintProvenance::new("40ohm", Some(1), ConstraintTier::Interface, "X"),
            ConstraintProvenance::new("50ohm", Some(2), ConstraintTier::Specific, "X"),
        ];
        assert!(!ConstraintProvenance::has_same_tier_conflict(&override_));

        // Same tier, same value (a wildcard hitting a pin twice) is not a
        // conflict.
        let dup = vec![
            ConstraintProvenance::new("40ohm", Some(1), ConstraintTier::Interface, "X"),
            ConstraintProvenance::new("40ohm", Some(2), ConstraintTier::Interface, "X"),
        ];
        assert!(!ConstraintProvenance::has_same_tier_conflict(&dup));
    }
}
