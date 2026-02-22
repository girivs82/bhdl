//! Rich parameterized type system for BHDL.
//!
//! Replaces string-based type representation with a structured algebraic type
//! that supports electrical primitives, signal types, composites, and parametric types.

use std::fmt;

/// The core type representation for BHDL.
///
/// Covers electrical primitives (with optional specs), signal types,
/// composite types, and parametric placeholders for generics.
#[derive(Debug, Clone, PartialEq)]
pub enum BhdlType {
    // --- Electrical primitives ---
    /// Voltage type, optionally parameterized with nominal and tolerance.
    Voltage(Option<VoltageSpec>),
    /// Current type.
    Current(Option<CurrentSpec>),
    /// Resistance type.
    Resistance(Option<ResistanceSpec>),
    /// Capacitance type.
    Capacitance,
    /// Inductance type.
    Inductance,
    /// Impedance type (complex).
    Impedance,
    /// Power type.
    Power,
    /// Frequency type.
    Frequency,
    /// Temperature type.
    Temperature,
    /// Time type.
    Time,

    // --- Signal types ---
    /// Single-bit digital signal, optionally associated with a voltage domain.
    Signal(Option<String>),
    /// Multi-bit bus with parameterized width.
    Bus(Width),
    /// Differential signal pair.
    Differential,

    // --- Power/Ground ---
    /// Power domain type (e.g., power<3.3V>).
    PowerDomain(Option<f64>),
    /// Ground reference.
    Ground,

    // --- Composite types ---
    /// Fixed-size array of a single element type.
    Array {
        element: Box<BhdlType>,
        size: ArraySize,
    },
    /// Named struct type (fields resolved separately).
    Struct(String),
    /// Named enum type (variants resolved separately).
    Enum(String),
    /// Named trait type (pins and consts resolved separately).
    Trait(String),

    // --- Parametric (for generics) ---
    /// Type parameter placeholder (e.g., `T` in `module Foo<T>`).
    TypeParam(String),
    /// Const parameter placeholder (e.g., `N` in `module Foo<N: nat>`).
    ConstParam(String),

    // --- Scalar value types ---
    /// Integer type (for generic const params like pin counts).
    Integer,
    /// Boolean type (for generic const params like feature flags).
    Bool,

    // --- Special ---
    /// Unknown or unresolved type (used during type inference).
    Unknown,
    /// Error sentinel (type checking failed).
    Error,
}

/// Bus width specification.
#[derive(Debug, Clone, PartialEq)]
pub enum Width {
    /// Fixed known width (e.g., `bus[8]`).
    Fixed(u32),
    /// Width from a const parameter (e.g., `bus[N]`).
    Param(String),
    /// Width to be inferred.
    Inferred,
}

/// Array size specification.
#[derive(Debug, Clone, PartialEq)]
pub enum ArraySize {
    /// Fixed known size (e.g., `T[4]`).
    Fixed(usize),
    /// Size from a const parameter (e.g., `T[N]`).
    Param(String),
}

/// Voltage specification with nominal value and tolerance.
#[derive(Debug, Clone, PartialEq)]
pub struct VoltageSpec {
    /// Nominal voltage in Volts.
    pub nominal: f64,
    /// Tolerance as a fraction (e.g., 0.05 = ±5%).
    pub tolerance: f64,
}

/// Current specification.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentSpec {
    /// Maximum current in Amps.
    pub max: f64,
}

/// Resistance specification.
#[derive(Debug, Clone, PartialEq)]
pub struct ResistanceSpec {
    /// Nominal resistance in Ohms.
    pub nominal: f64,
    /// Tolerance as a fraction.
    pub tolerance: f64,
}

impl BhdlType {
    /// Create a voltage type with nominal and tolerance.
    pub fn voltage_with_spec(nominal: f64, tolerance: f64) -> Self {
        BhdlType::Voltage(Some(VoltageSpec { nominal, tolerance }))
    }

    /// Create a fixed-width bus type.
    pub fn bus(width: u32) -> Self {
        BhdlType::Bus(Width::Fixed(width))
    }

    /// Create a parameterized bus type.
    pub fn bus_param(name: impl Into<String>) -> Self {
        BhdlType::Bus(Width::Param(name.into()))
    }

    /// Create a fixed-size array type.
    pub fn array(element: BhdlType, size: usize) -> Self {
        BhdlType::Array {
            element: Box::new(element),
            size: ArraySize::Fixed(size),
        }
    }

    /// Whether this is an electrical quantity type.
    pub fn is_electrical(&self) -> bool {
        matches!(
            self,
            BhdlType::Voltage(_)
                | BhdlType::Current(_)
                | BhdlType::Resistance(_)
                | BhdlType::Capacitance
                | BhdlType::Inductance
                | BhdlType::Impedance
                | BhdlType::Power
                | BhdlType::Frequency
                | BhdlType::Temperature
                | BhdlType::Time
        )
    }

    /// Whether this is a signal-carrying type.
    pub fn is_signal(&self) -> bool {
        matches!(
            self,
            BhdlType::Signal(_) | BhdlType::Bus(_) | BhdlType::Differential
        )
    }

    /// Whether this is a power/ground type.
    pub fn is_power_or_ground(&self) -> bool {
        matches!(self, BhdlType::PowerDomain(_) | BhdlType::Ground)
    }

    /// Whether this type contains unresolved parameters.
    pub fn has_params(&self) -> bool {
        match self {
            BhdlType::TypeParam(_) | BhdlType::ConstParam(_) => true,
            BhdlType::Bus(Width::Param(_)) => true,
            BhdlType::Array { element, size } => {
                element.has_params() || matches!(size, ArraySize::Param(_))
            }
            _ => false,
        }
    }

    /// Get bus width if this is a fixed-width bus.
    pub fn bus_width(&self) -> Option<u32> {
        match self {
            BhdlType::Bus(Width::Fixed(w)) => Some(*w),
            _ => None,
        }
    }

    /// Parse a base type name string into a BhdlType.
    /// This provides backward compatibility with the string-based type system.
    pub fn from_type_name(name: &str, bounds: Option<(i64, i64)>) -> Self {
        let base = match name {
            "signal" => BhdlType::Signal(None),
            "power" => BhdlType::PowerDomain(None),
            "ground" => BhdlType::Ground,
            "voltage" => BhdlType::Voltage(None),
            "current" => BhdlType::Current(None),
            "resistance" => BhdlType::Resistance(None),
            "capacitance" => BhdlType::Capacitance,
            "inductance" => BhdlType::Inductance,
            "impedance" => BhdlType::Impedance,
            "frequency" => BhdlType::Frequency,
            "temperature" => BhdlType::Temperature,
            "time" => BhdlType::Time,
            "differential" => BhdlType::Differential,
            "integer" | "int" => BhdlType::Integer,
            "bool" | "boolean" => BhdlType::Bool,
            _ => BhdlType::Unknown,
        };

        // If there are bus bounds, wrap in a Bus type
        if let Some((high, low)) = bounds {
            let width = (high - low).unsigned_abs() as u32 + 1;
            match base {
                BhdlType::Signal(_) => BhdlType::Bus(Width::Fixed(width)),
                other => other,
            }
        } else {
            base
        }
    }
}

impl fmt::Display for BhdlType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BhdlType::Voltage(None) => write!(f, "voltage"),
            BhdlType::Voltage(Some(spec)) => {
                write!(f, "voltage<{}, {}>", spec.nominal, spec.tolerance)
            }
            BhdlType::Current(None) => write!(f, "current"),
            BhdlType::Current(Some(spec)) => write!(f, "current<{}>", spec.max),
            BhdlType::Resistance(None) => write!(f, "resistance"),
            BhdlType::Resistance(Some(spec)) => {
                write!(f, "resistance<{}, {}>", spec.nominal, spec.tolerance)
            }
            BhdlType::Capacitance => write!(f, "capacitance"),
            BhdlType::Inductance => write!(f, "inductance"),
            BhdlType::Impedance => write!(f, "impedance"),
            BhdlType::Power => write!(f, "power"),
            BhdlType::Frequency => write!(f, "frequency"),
            BhdlType::Temperature => write!(f, "temperature"),
            BhdlType::Time => write!(f, "time"),
            BhdlType::Signal(None) => write!(f, "signal"),
            BhdlType::Signal(Some(domain)) => write!(f, "signal<@{}>", domain),
            BhdlType::Bus(Width::Fixed(w)) => write!(f, "bus[{}]", w),
            BhdlType::Bus(Width::Param(p)) => write!(f, "bus[{}]", p),
            BhdlType::Bus(Width::Inferred) => write!(f, "bus[_]"),
            BhdlType::Differential => write!(f, "differential"),
            BhdlType::PowerDomain(None) => write!(f, "power"),
            BhdlType::PowerDomain(Some(v)) => write!(f, "power<{}V>", v),
            BhdlType::Ground => write!(f, "ground"),
            BhdlType::Array { element, size } => match size {
                ArraySize::Fixed(n) => write!(f, "{}[{}]", element, n),
                ArraySize::Param(p) => write!(f, "{}[{}]", element, p),
            },
            BhdlType::Struct(name) => write!(f, "struct {}", name),
            BhdlType::Enum(name) => write!(f, "enum {}", name),
            BhdlType::Trait(name) => write!(f, "trait {}", name),
            BhdlType::TypeParam(name) => write!(f, "{}", name),
            BhdlType::ConstParam(name) => write!(f, "{}", name),
            BhdlType::Integer => write!(f, "integer"),
            BhdlType::Bool => write!(f, "bool"),
            BhdlType::Unknown => write!(f, "unknown"),
            BhdlType::Error => write!(f, "<error>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_type_name_basic() {
        assert_eq!(BhdlType::from_type_name("signal", None), BhdlType::Signal(None));
        assert_eq!(BhdlType::from_type_name("voltage", None), BhdlType::Voltage(None));
        assert_eq!(BhdlType::from_type_name("ground", None), BhdlType::Ground);
        assert_eq!(BhdlType::from_type_name("unknown_type", None), BhdlType::Unknown);
    }

    #[test]
    fn test_from_type_name_bus() {
        // signal with bounds becomes a bus
        assert_eq!(
            BhdlType::from_type_name("signal", Some((7, 0))),
            BhdlType::Bus(Width::Fixed(8))
        );
    }

    #[test]
    fn test_voltage_with_spec() {
        let t = BhdlType::voltage_with_spec(3.3, 0.05);
        assert_eq!(
            format!("{}", t),
            "voltage<3.3, 0.05>"
        );
        assert!(t.is_electrical());
        assert!(!t.is_signal());
    }

    #[test]
    fn test_bus_width() {
        let b = BhdlType::bus(8);
        assert_eq!(b.bus_width(), Some(8));
        assert!(b.is_signal());
        assert!(!b.is_electrical());
    }

    #[test]
    fn test_has_params() {
        assert!(!BhdlType::Signal(None).has_params());
        assert!(BhdlType::TypeParam("T".into()).has_params());
        assert!(BhdlType::bus_param("N").has_params());
        assert!(BhdlType::Array {
            element: Box::new(BhdlType::Signal(None)),
            size: ArraySize::Param("N".into()),
        }.has_params());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", BhdlType::Signal(None)), "signal");
        assert_eq!(format!("{}", BhdlType::bus(16)), "bus[16]");
        assert_eq!(format!("{}", BhdlType::Ground), "ground");
        assert_eq!(
            format!("{}", BhdlType::array(BhdlType::Signal(None), 4)),
            "signal[4]"
        );
    }

    #[test]
    fn test_integer_and_bool_types() {
        assert_eq!(BhdlType::from_type_name("integer", None), BhdlType::Integer);
        assert_eq!(BhdlType::from_type_name("int", None), BhdlType::Integer);
        assert_eq!(BhdlType::from_type_name("bool", None), BhdlType::Bool);
        assert_eq!(BhdlType::from_type_name("boolean", None), BhdlType::Bool);
        assert!(!BhdlType::Integer.is_electrical());
        assert!(!BhdlType::Bool.is_electrical());
        assert!(!BhdlType::Integer.is_signal());
        assert!(!BhdlType::Bool.is_signal());
        assert_eq!(format!("{}", BhdlType::Integer), "integer");
        assert_eq!(format!("{}", BhdlType::Bool), "bool");
    }
}
