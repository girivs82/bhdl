//! Name resolution for intent lowering.
//!
//! `LayoutIntent` carries name-based references — `PinRef::HostPin("VCC")`
//! resolves against the *host* entity's pins (the parent component an
//! expansion child was born from), and `PinRef::BoardPin { component,
//! pin }` resolves against a named board component. This module builds
//! lookup tables over a `Board` and resolves those references to
//! `PinSel { component, pin }`.

use crate::det::HashMap;

use bhdl_common::intent::vocabulary::{ComponentRef, PinRef};

use crate::constraint::PinSel;
use crate::types::{Board, ComponentId, PinId};

/// Resolution context built once per board.
pub struct LoweringContext {
    /// component name → id
    by_name: HashMap<String, ComponentId>,
    /// (component id, pin name) → pin id
    pins: HashMap<(ComponentId, String), PinId>,
    /// component id → its host (parent) component id, if it was born from
    /// an expansion. Reconstructed from functional-group parent links.
    host_of: HashMap<ComponentId, ComponentId>,
    /// component id → ordered pin ids (for positional self-pin access)
    ordered_pins: HashMap<ComponentId, Vec<PinId>>,
}

impl LoweringContext {
    pub fn build(board: &Board) -> Self {
        let mut by_name = HashMap::default();
        let mut pins = HashMap::default();
        let mut ordered_pins = HashMap::default();

        for c in &board.components {
            by_name.insert(c.name.clone(), c.id);
            let mut order = Vec::with_capacity(c.pins.len());
            for p in &c.pins {
                pins.insert((c.id, p.name.clone()), p.pin_id);
                order.push(p.pin_id);
            }
            ordered_pins.insert(c.id, order);
        }

        // Host relationship: a group's `parent` is the host of every
        // member. (Decoupling caps born from an MCU's expansion land in a
        // functional group whose parent is the MCU.)
        let mut host_of = HashMap::default();
        for g in &board.groups {
            if let Some(parent) = g.parent {
                for &m in &g.members {
                    if m != parent {
                        host_of.insert(m, parent);
                    }
                }
            }
        }

        LoweringContext { by_name, pins, host_of, ordered_pins }
    }

    pub fn component_by_name(&self, name: &str) -> Option<ComponentId> {
        self.by_name.get(name).copied()
    }

    pub fn host_of(&self, c: ComponentId) -> Option<ComponentId> {
        self.host_of.get(&c).copied()
    }

    fn pin_by_name(&self, c: ComponentId, name: &str) -> Option<PinId> {
        self.pins.get(&(c, name.to_string())).copied()
    }

    /// The nth pin (0-based) of a component, for positional self-pin
    /// access (`self.pin1` / `self.pin2`).
    pub fn self_pin(&self, c: ComponentId, index: usize) -> Option<PinSel> {
        self.ordered_pins
            .get(&c)
            .and_then(|v| v.get(index))
            .map(|&pin| PinSel { component: c, pin })
    }

    /// Resolve a `PinRef` for an intent attached to `self_component`.
    ///
    /// - `HostPin(name)` → the pin of `self_component`'s host entity.
    /// - `BoardPin { component, pin }` → the named board component's pin.
    ///
    /// Returns `None` (and the caller drops the constraint with a warning)
    /// if the host is unknown or the pin name doesn't exist — per the
    /// "string-now, resolve-later; typo surfaces as a lowering error"
    /// contract (handshake §8.3).
    pub fn resolve_pin(
        &self,
        self_component: ComponentId,
        pin_ref: &PinRef,
    ) -> Result<PinSel, ResolveError> {
        match pin_ref {
            PinRef::HostPin(name) => {
                let host = self
                    .host_of(self_component)
                    .ok_or(ResolveError::NoHost { self_component })?;
                let pin = self.pin_by_name(host, name).ok_or_else(|| {
                    ResolveError::NoPin { component: host, pin: name.clone() }
                })?;
                Ok(PinSel { component: host, pin })
            }
            PinRef::BoardPin { component, pin } => {
                let cid = self.component_by_name(component).ok_or_else(|| {
                    ResolveError::NoComponent { name: component.clone() }
                })?;
                let pid = self.pin_by_name(cid, pin).ok_or_else(|| {
                    ResolveError::NoPin { component: cid, pin: pin.clone() }
                })?;
                Ok(PinSel { component: cid, pin: pid })
            }
        }
    }

    /// Resolve a sibling `ComponentRef` within the same expansion (e.g. a
    /// crystal load cap's partner). Looked up by name at board scope.
    pub fn resolve_component(
        &self,
        cref: &ComponentRef,
    ) -> Result<ComponentId, ResolveError> {
        self.component_by_name(&cref.0)
            .ok_or_else(|| ResolveError::NoComponent { name: cref.0.clone() })
    }
}

/// Why a name reference failed to resolve. Surfaced as a lowering-time
/// diagnostic; the offending constraint is dropped (warn-and-degrade).
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveError {
    /// An expansion child carries a `HostPin` ref but no host was found.
    NoHost { self_component: ComponentId },
    /// A component name in a `BoardPin` / `ComponentRef` didn't resolve.
    NoComponent { name: String },
    /// A pin name didn't exist on the resolved component.
    NoPin { component: ComponentId, pin: String },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NoHost { .. } => write!(
                f,
                "intent references a host pin but the component has no \
                 resolvable host (parent) — was it born from an expansion?"
            ),
            ResolveError::NoComponent { name } => {
                write!(f, "no component named '{name}' on the board")
            }
            ResolveError::NoPin { pin, .. } => {
                write!(f, "no pin named '{pin}' on the resolved component")
            }
        }
    }
}
