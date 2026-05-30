//! Intent → constraint lowering for `bhdl-pnr`.
//!
//! Two producers feed the constraint catalog (`constraint_model_v0.md`
//! §1):
//!   - **Expansion intent** — typed `LayoutIntent` values on components,
//!     lowered by per-kind recipes (`recipes`). This module.
//!   - **Interface constraints** — `intf_const__*` module attributes,
//!     parsed by the boundary reader (`interface_constraints`, TODO).
//!
//! The `lowering` driver runs both and populates `Board.constraints`.
//!
//! Name resolution: `LayoutIntent` carries *name-based* pin references
//! (`PinRef::HostPin("VCC")`). The `resolve` module turns those into
//! *resolved* `PinSel { component, pin }` selectors against the built
//! board, so the constraint catalog is fully resolved and `eval`-able.

pub mod interface_constraints;
pub mod lowering;
pub mod recipes;
pub mod resolve;

pub use lowering::lower_board_intents;
