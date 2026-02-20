//! Rich constant value type for BHDL compile-time evaluation.
//!
//! Supports integers, floats, booleans, strings, and physical quantities
//! (voltage, current, resistance, capacitance, inductance, power, frequency, time).
//! Physical quantities are stored in base SI units internally.

use std::fmt;

/// Compile-time constant value with support for physical quantities.
///
/// All physical quantities store values in base SI units:
/// - Voltage in Volts, Current in Amps, Resistance in Ohms
/// - Capacitance in Farads, Inductance in Henries
/// - Power in Watts, Frequency in Hertz, Time in Seconds
#[derive(Debug, Clone)]
pub enum ConstValue {
    // Scalar types
    Integer(i64),
    Float(f64),
    Bool(bool),
    String(String),

    // Physical quantities (base SI units)
    Voltage(f64),
    Current(f64),
    Resistance(f64),
    Capacitance(f64),
    Inductance(f64),
    Power(f64),
    Frequency(f64),
    Time(f64),
}

/// Dimensional exponent vector for physical quantities.
///
/// Each field is a signed exponent. For example:
/// - Voltage = [V^1, A^0, s^0, °C^0, m^0]
/// - Power = V * A = [V^1, A^1, s^0, °C^0, m^0]
/// - Resistance = V / A = [V^1, A^-1, s^0, °C^0, m^0]
/// - Capacitance = A * s / V = [V^-1, A^1, s^1, °C^0, m^0]
/// - Inductance = V * s / A = [V^1, A^-1, s^1, °C^0, m^0]
/// - Frequency = 1/s = [V^0, A^0, s^-1, °C^0, m^0]
/// - Time = [V^0, A^0, s^1, °C^0, m^0]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dimension {
    pub voltage: i8,
    pub current: i8,
    pub time: i8,
    pub temperature: i8,
    pub length: i8,
}

impl Dimension {
    pub const DIMENSIONLESS: Self = Dimension { voltage: 0, current: 0, time: 0, temperature: 0, length: 0 };
    pub const VOLTAGE: Self       = Dimension { voltage: 1, current: 0, time: 0, temperature: 0, length: 0 };
    pub const CURRENT: Self       = Dimension { voltage: 0, current: 1, time: 0, temperature: 0, length: 0 };
    pub const RESISTANCE: Self    = Dimension { voltage: 1, current: -1, time: 0, temperature: 0, length: 0 };
    pub const CAPACITANCE: Self   = Dimension { voltage: -1, current: 1, time: 1, temperature: 0, length: 0 };
    pub const INDUCTANCE: Self    = Dimension { voltage: 1, current: -1, time: 1, temperature: 0, length: 0 };
    pub const POWER: Self         = Dimension { voltage: 1, current: 1, time: 0, temperature: 0, length: 0 };
    pub const FREQUENCY: Self     = Dimension { voltage: 0, current: 0, time: -1, temperature: 0, length: 0 };
    pub const TIME: Self          = Dimension { voltage: 0, current: 0, time: 1, temperature: 0, length: 0 };

    /// Multiply dimensions (add exponents).
    pub fn mul(self, rhs: Dimension) -> Dimension {
        Dimension {
            voltage: self.voltage + rhs.voltage,
            current: self.current + rhs.current,
            time: self.time + rhs.time,
            temperature: self.temperature + rhs.temperature,
            length: self.length + rhs.length,
        }
    }

    /// Divide dimensions (subtract exponents).
    pub fn div(self, rhs: Dimension) -> Dimension {
        Dimension {
            voltage: self.voltage - rhs.voltage,
            current: self.current - rhs.current,
            time: self.time - rhs.time,
            temperature: self.temperature - rhs.temperature,
            length: self.length - rhs.length,
        }
    }

    /// Negate all exponents (for reciprocal).
    pub fn reciprocal(self) -> Dimension {
        Dimension {
            voltage: -self.voltage,
            current: -self.current,
            time: -self.time,
            temperature: -self.temperature,
            length: -self.length,
        }
    }

    /// Check if this is a dimensionless quantity.
    pub fn is_dimensionless(&self) -> bool {
        *self == Self::DIMENSIONLESS
    }

    /// Human-readable name for this dimension, or None if not a recognized standard dimension.
    pub fn name(&self) -> Option<&'static str> {
        match *self {
            Self::DIMENSIONLESS => Some("dimensionless"),
            Self::VOLTAGE => Some("voltage"),
            Self::CURRENT => Some("current"),
            Self::RESISTANCE => Some("resistance"),
            Self::CAPACITANCE => Some("capacitance"),
            Self::INDUCTANCE => Some("inductance"),
            Self::POWER => Some("power"),
            Self::FREQUENCY => Some("frequency"),
            Self::TIME => Some("time"),
            _ => None,
        }
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_dimensionless() {
            return write!(f, "dimensionless");
        }
        if let Some(name) = self.name() {
            return write!(f, "{}", name);
        }
        // Fallback: show exponent notation
        let mut parts = Vec::new();
        if self.voltage != 0 { parts.push(format!("V^{}", self.voltage)); }
        if self.current != 0 { parts.push(format!("A^{}", self.current)); }
        if self.time != 0 { parts.push(format!("s^{}", self.time)); }
        if self.temperature != 0 { parts.push(format!("°C^{}", self.temperature)); }
        if self.length != 0 { parts.push(format!("m^{}", self.length)); }
        write!(f, "[{}]", parts.join("·"))
    }
}

/// Map a Dimension back to the appropriate ConstValue constructor.
/// Returns None for unrecognized dimension combinations.
fn dimension_to_constructor(dim: &Dimension) -> Option<fn(f64) -> ConstValue> {
    match *dim {
        Dimension::VOLTAGE => Some(ConstValue::Voltage),
        Dimension::CURRENT => Some(ConstValue::Current),
        Dimension::RESISTANCE => Some(ConstValue::Resistance),
        Dimension::CAPACITANCE => Some(ConstValue::Capacitance),
        Dimension::INDUCTANCE => Some(ConstValue::Inductance),
        Dimension::POWER => Some(ConstValue::Power),
        Dimension::FREQUENCY => Some(ConstValue::Frequency),
        Dimension::TIME => Some(ConstValue::Time),
        Dimension::DIMENSIONLESS => None, // Use Float for dimensionless
        _ => None,
    }
}

/// Error type for constant expression evaluation.
#[derive(Debug, Clone)]
pub enum EvalError {
    NotConstant(std::string::String),
    UndefinedSymbol(std::string::String),
    TypeMismatch(std::string::String),
    DivisionByZero,
    Overflow(std::string::String),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::NotConstant(s) => write!(f, "not a constant expression: {}", s),
            EvalError::UndefinedSymbol(s) => write!(f, "undefined symbol: {}", s),
            EvalError::TypeMismatch(s) => write!(f, "type mismatch: {}", s),
            EvalError::DivisionByZero => write!(f, "division by zero"),
            EvalError::Overflow(s) => write!(f, "overflow: {}", s),
        }
    }
}

// Custom PartialEq: use to_bits() for float comparison to handle NaN correctly.
impl PartialEq for ConstValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ConstValue::Integer(a), ConstValue::Integer(b)) => a == b,
            (ConstValue::Float(a), ConstValue::Float(b)) => a.to_bits() == b.to_bits(),
            (ConstValue::Bool(a), ConstValue::Bool(b)) => a == b,
            (ConstValue::String(a), ConstValue::String(b)) => a == b,
            (ConstValue::Voltage(a), ConstValue::Voltage(b)) => a.to_bits() == b.to_bits(),
            (ConstValue::Current(a), ConstValue::Current(b)) => a.to_bits() == b.to_bits(),
            (ConstValue::Resistance(a), ConstValue::Resistance(b)) => a.to_bits() == b.to_bits(),
            (ConstValue::Capacitance(a), ConstValue::Capacitance(b)) => a.to_bits() == b.to_bits(),
            (ConstValue::Inductance(a), ConstValue::Inductance(b)) => a.to_bits() == b.to_bits(),
            (ConstValue::Power(a), ConstValue::Power(b)) => a.to_bits() == b.to_bits(),
            (ConstValue::Frequency(a), ConstValue::Frequency(b)) => a.to_bits() == b.to_bits(),
            (ConstValue::Time(a), ConstValue::Time(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

impl ConstValue {
    /// Extract as i64 if this is an Integer value.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            ConstValue::Integer(n) => Some(*n),
            _ => None,
        }
    }

    /// Extract the underlying f64 for any numeric value (including physical quantities).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ConstValue::Integer(n) => Some(*n as f64),
            ConstValue::Float(f) => Some(*f),
            ConstValue::Voltage(v) => Some(*v),
            ConstValue::Current(a) => Some(*a),
            ConstValue::Resistance(r) => Some(*r),
            ConstValue::Capacitance(c) => Some(*c),
            ConstValue::Inductance(l) => Some(*l),
            ConstValue::Power(w) => Some(*w),
            ConstValue::Frequency(hz) => Some(*hz),
            ConstValue::Time(s) => Some(*s),
            _ => None,
        }
    }

    /// Whether this value is a physical quantity (has units).
    pub fn is_physical(&self) -> bool {
        matches!(
            self,
            ConstValue::Voltage(_)
                | ConstValue::Current(_)
                | ConstValue::Resistance(_)
                | ConstValue::Capacitance(_)
                | ConstValue::Inductance(_)
                | ConstValue::Power(_)
                | ConstValue::Frequency(_)
                | ConstValue::Time(_)
        )
    }

    /// Get the dimensional exponent vector for this value, or None for non-physical types.
    pub fn dimension(&self) -> Option<Dimension> {
        match self {
            ConstValue::Voltage(_) => Some(Dimension::VOLTAGE),
            ConstValue::Current(_) => Some(Dimension::CURRENT),
            ConstValue::Resistance(_) => Some(Dimension::RESISTANCE),
            ConstValue::Capacitance(_) => Some(Dimension::CAPACITANCE),
            ConstValue::Inductance(_) => Some(Dimension::INDUCTANCE),
            ConstValue::Power(_) => Some(Dimension::POWER),
            ConstValue::Frequency(_) => Some(Dimension::FREQUENCY),
            ConstValue::Time(_) => Some(Dimension::TIME),
            _ => None,
        }
    }

    /// Get a human-readable unit suffix.
    pub fn unit_suffix(&self) -> &'static str {
        match self {
            ConstValue::Voltage(_) => "V",
            ConstValue::Current(_) => "A",
            ConstValue::Resistance(_) => "Ω",
            ConstValue::Capacitance(_) => "F",
            ConstValue::Inductance(_) => "H",
            ConstValue::Power(_) => "W",
            ConstValue::Frequency(_) => "Hz",
            ConstValue::Time(_) => "s",
            _ => "",
        }
    }

    /// Short name for the physical dimension (for diagnostics).
    pub fn dimension_name(&self) -> &'static str {
        match self {
            ConstValue::Integer(_) => "integer",
            ConstValue::Float(_) => "float",
            ConstValue::Bool(_) => "bool",
            ConstValue::String(_) => "string",
            ConstValue::Voltage(_) => "voltage",
            ConstValue::Current(_) => "current",
            ConstValue::Resistance(_) => "resistance",
            ConstValue::Capacitance(_) => "capacitance",
            ConstValue::Inductance(_) => "inductance",
            ConstValue::Power(_) => "power",
            ConstValue::Frequency(_) => "frequency",
            ConstValue::Time(_) => "time",
        }
    }

    /// Negate the value (unary minus).
    pub fn negate(self) -> Result<ConstValue, EvalError> {
        match self {
            ConstValue::Integer(n) => n
                .checked_neg()
                .map(ConstValue::Integer)
                .ok_or_else(|| EvalError::Overflow("integer negation overflow".into())),
            ConstValue::Float(f) => Ok(ConstValue::Float(-f)),
            ConstValue::Voltage(v) => Ok(ConstValue::Voltage(-v)),
            ConstValue::Current(a) => Ok(ConstValue::Current(-a)),
            ConstValue::Resistance(r) => Ok(ConstValue::Resistance(-r)),
            ConstValue::Capacitance(c) => Ok(ConstValue::Capacitance(-c)),
            ConstValue::Inductance(l) => Ok(ConstValue::Inductance(-l)),
            ConstValue::Power(w) => Ok(ConstValue::Power(-w)),
            ConstValue::Frequency(hz) => Ok(ConstValue::Frequency(-hz)),
            ConstValue::Time(s) => Ok(ConstValue::Time(-s)),
            other => Err(EvalError::TypeMismatch(format!(
                "cannot negate {}",
                other.dimension_name()
            ))),
        }
    }

    /// Add two values. Same-dimension physical quantities can be added.
    pub fn add(self, rhs: ConstValue) -> Result<ConstValue, EvalError> {
        match (self, rhs) {
            // Integer + Integer
            (ConstValue::Integer(a), ConstValue::Integer(b)) => a
                .checked_add(b)
                .map(ConstValue::Integer)
                .ok_or_else(|| EvalError::Overflow("integer addition overflow".into())),
            // Float + Float (or Integer promoted to Float)
            (ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a + b)),
            (ConstValue::Integer(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a as f64 + b)),
            (ConstValue::Float(a), ConstValue::Integer(b)) => Ok(ConstValue::Float(a + b as f64)),
            // Same-dimension physical quantities
            (ConstValue::Voltage(a), ConstValue::Voltage(b)) => Ok(ConstValue::Voltage(a + b)),
            (ConstValue::Current(a), ConstValue::Current(b)) => Ok(ConstValue::Current(a + b)),
            (ConstValue::Resistance(a), ConstValue::Resistance(b)) => {
                Ok(ConstValue::Resistance(a + b))
            }
            (ConstValue::Capacitance(a), ConstValue::Capacitance(b)) => {
                Ok(ConstValue::Capacitance(a + b))
            }
            (ConstValue::Inductance(a), ConstValue::Inductance(b)) => {
                Ok(ConstValue::Inductance(a + b))
            }
            (ConstValue::Power(a), ConstValue::Power(b)) => Ok(ConstValue::Power(a + b)),
            (ConstValue::Frequency(a), ConstValue::Frequency(b)) => {
                Ok(ConstValue::Frequency(a + b))
            }
            (ConstValue::Time(a), ConstValue::Time(b)) => Ok(ConstValue::Time(a + b)),
            // Mismatched dimensions
            (a, b) => Err(EvalError::TypeMismatch(format!(
                "cannot add {} and {}",
                a.dimension_name(),
                b.dimension_name()
            ))),
        }
    }

    /// Subtract two values. Same-dimension physical quantities can be subtracted.
    pub fn sub(self, rhs: ConstValue) -> Result<ConstValue, EvalError> {
        match (self, rhs) {
            (ConstValue::Integer(a), ConstValue::Integer(b)) => a
                .checked_sub(b)
                .map(ConstValue::Integer)
                .ok_or_else(|| EvalError::Overflow("integer subtraction overflow".into())),
            (ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a - b)),
            (ConstValue::Integer(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a as f64 - b)),
            (ConstValue::Float(a), ConstValue::Integer(b)) => Ok(ConstValue::Float(a - b as f64)),
            (ConstValue::Voltage(a), ConstValue::Voltage(b)) => Ok(ConstValue::Voltage(a - b)),
            (ConstValue::Current(a), ConstValue::Current(b)) => Ok(ConstValue::Current(a - b)),
            (ConstValue::Resistance(a), ConstValue::Resistance(b)) => {
                Ok(ConstValue::Resistance(a - b))
            }
            (ConstValue::Capacitance(a), ConstValue::Capacitance(b)) => {
                Ok(ConstValue::Capacitance(a - b))
            }
            (ConstValue::Inductance(a), ConstValue::Inductance(b)) => {
                Ok(ConstValue::Inductance(a - b))
            }
            (ConstValue::Power(a), ConstValue::Power(b)) => Ok(ConstValue::Power(a - b)),
            (ConstValue::Frequency(a), ConstValue::Frequency(b)) => {
                Ok(ConstValue::Frequency(a - b))
            }
            (ConstValue::Time(a), ConstValue::Time(b)) => Ok(ConstValue::Time(a - b)),
            (a, b) => Err(EvalError::TypeMismatch(format!(
                "cannot subtract {} from {}",
                b.dimension_name(),
                a.dimension_name()
            ))),
        }
    }

    /// Multiply two values with generalized dimensional analysis.
    ///
    /// Dimension exponents are added: V * A → [V^1·A^1] = Power,
    /// R * C → [V^1·A^-1] * [V^-1·A^1·s^1] = [s^1] = Time, etc.
    /// Scalar * Physical = Physical (scaling).
    pub fn mul(self, rhs: ConstValue) -> Result<ConstValue, EvalError> {
        match (&self, &rhs) {
            // Pure integer arithmetic
            (ConstValue::Integer(a), ConstValue::Integer(b)) => {
                return a
                    .checked_mul(*b)
                    .map(ConstValue::Integer)
                    .ok_or_else(|| EvalError::Overflow("integer multiplication overflow".into()));
            }
            // Float * Float
            (ConstValue::Float(a), ConstValue::Float(b)) => return Ok(ConstValue::Float(a * b)),
            (ConstValue::Integer(a), ConstValue::Float(b)) => {
                return Ok(ConstValue::Float(*a as f64 * b));
            }
            (ConstValue::Float(a), ConstValue::Integer(b)) => {
                return Ok(ConstValue::Float(a * *b as f64));
            }
            _ => {}
        }

        // Scalar * Physical (scaling preserves dimension)
        let (scalar, phys) = match (&self, &rhs) {
            (ConstValue::Integer(n), p) if p.is_physical() => (Some(*n as f64), Some(&rhs)),
            (p, ConstValue::Integer(n)) if p.is_physical() => (Some(*n as f64), Some(&self)),
            (ConstValue::Float(f), p) if p.is_physical() => (Some(*f), Some(&rhs)),
            (p, ConstValue::Float(f)) if p.is_physical() => (Some(*f), Some(&self)),
            _ => (None, None),
        };
        if let (Some(s), Some(p)) = (scalar, phys) {
            let scaled = p.as_f64().unwrap() * s;
            return Ok(p.with_value(scaled));
        }

        // Physical * Physical: generalized dimensional multiplication
        if let (Some(dim_a), Some(dim_b)) = (self.dimension(), rhs.dimension()) {
            let val = self.as_f64().unwrap() * rhs.as_f64().unwrap();
            let result_dim = dim_a.mul(dim_b);
            if result_dim.is_dimensionless() {
                return Ok(ConstValue::Float(val));
            }
            if let Some(ctor) = dimension_to_constructor(&result_dim) {
                return Ok(ctor(val));
            }
            return Err(EvalError::TypeMismatch(format!(
                "multiplication produces unrecognized dimension {}",
                result_dim
            )));
        }

        Err(EvalError::TypeMismatch(format!(
            "cannot multiply {} by {}",
            self.dimension_name(),
            rhs.dimension_name()
        )))
    }

    /// Divide two values with generalized dimensional analysis.
    ///
    /// Dimension exponents are subtracted: V / A → [V^1·A^-1] = Resistance,
    /// V / R → [V^1] / [V^1·A^-1] = [A^1] = Current, etc.
    /// Physical / Scalar = Physical (scaling).
    /// Same dim / Same dim = Float (dimensionless ratio).
    pub fn div(self, rhs: ConstValue) -> Result<ConstValue, EvalError> {
        // Check division by zero for all numeric types
        match &rhs {
            ConstValue::Integer(0) => return Err(EvalError::DivisionByZero),
            v if v.as_f64() == Some(0.0) => return Err(EvalError::DivisionByZero),
            _ => {}
        }

        match (&self, &rhs) {
            // Pure integer arithmetic
            (ConstValue::Integer(a), ConstValue::Integer(b)) => {
                return Ok(ConstValue::Integer(a / b));
            }
            (ConstValue::Float(a), ConstValue::Float(b)) => return Ok(ConstValue::Float(a / b)),
            (ConstValue::Integer(a), ConstValue::Float(b)) => {
                return Ok(ConstValue::Float(*a as f64 / b));
            }
            (ConstValue::Float(a), ConstValue::Integer(b)) => {
                return Ok(ConstValue::Float(a / *b as f64));
            }
            _ => {}
        }

        // Physical / Scalar (scaling preserves dimension)
        match &rhs {
            ConstValue::Integer(n) if self.is_physical() => {
                let scaled = self.as_f64().unwrap() / *n as f64;
                return Ok(self.with_value(scaled));
            }
            ConstValue::Float(f) if self.is_physical() => {
                let scaled = self.as_f64().unwrap() / f;
                return Ok(self.with_value(scaled));
            }
            _ => {}
        }

        // Scalar / Physical: dimensionless / dim = inverse dim
        match &self {
            ConstValue::Integer(n) if rhs.is_physical() => {
                let val = *n as f64 / rhs.as_f64().unwrap();
                let result_dim = rhs.dimension().unwrap().reciprocal();
                if result_dim.is_dimensionless() {
                    return Ok(ConstValue::Float(val));
                }
                if let Some(ctor) = dimension_to_constructor(&result_dim) {
                    return Ok(ctor(val));
                }
                return Err(EvalError::TypeMismatch(format!(
                    "division produces unrecognized dimension {}",
                    result_dim
                )));
            }
            ConstValue::Float(f) if rhs.is_physical() => {
                let val = f / rhs.as_f64().unwrap();
                let result_dim = rhs.dimension().unwrap().reciprocal();
                if result_dim.is_dimensionless() {
                    return Ok(ConstValue::Float(val));
                }
                if let Some(ctor) = dimension_to_constructor(&result_dim) {
                    return Ok(ctor(val));
                }
                return Err(EvalError::TypeMismatch(format!(
                    "division produces unrecognized dimension {}",
                    result_dim
                )));
            }
            _ => {}
        }

        // Physical / Physical: generalized dimensional division
        if let (Some(dim_a), Some(dim_b)) = (self.dimension(), rhs.dimension()) {
            let val = self.as_f64().unwrap() / rhs.as_f64().unwrap();
            let result_dim = dim_a.div(dim_b);
            if result_dim.is_dimensionless() {
                return Ok(ConstValue::Float(val));
            }
            if let Some(ctor) = dimension_to_constructor(&result_dim) {
                return Ok(ctor(val));
            }
            return Err(EvalError::TypeMismatch(format!(
                "division produces unrecognized dimension {}",
                result_dim
            )));
        }

        Err(EvalError::TypeMismatch(format!(
            "cannot divide {} by {}",
            self.dimension_name(),
            rhs.dimension_name()
        )))
    }

    /// Create a new value of the same physical dimension with a different magnitude.
    fn with_value(&self, val: f64) -> ConstValue {
        match self {
            ConstValue::Voltage(_) => ConstValue::Voltage(val),
            ConstValue::Current(_) => ConstValue::Current(val),
            ConstValue::Resistance(_) => ConstValue::Resistance(val),
            ConstValue::Capacitance(_) => ConstValue::Capacitance(val),
            ConstValue::Inductance(_) => ConstValue::Inductance(val),
            ConstValue::Power(_) => ConstValue::Power(val),
            ConstValue::Frequency(_) => ConstValue::Frequency(val),
            ConstValue::Time(_) => ConstValue::Time(val),
            ConstValue::Float(_) => ConstValue::Float(val),
            _ => ConstValue::Float(val),
        }
    }
}

impl fmt::Display for ConstValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstValue::Integer(n) => write!(f, "{}", n),
            ConstValue::Float(v) => write!(f, "{}", v),
            ConstValue::Bool(b) => write!(f, "{}", b),
            ConstValue::String(s) => write!(f, "\"{}\"", s),
            ConstValue::Voltage(v) => write!(f, "{}{}", format_si_value(*v), "V"),
            ConstValue::Current(a) => write!(f, "{}{}", format_si_value(*a), "A"),
            ConstValue::Resistance(r) => write!(f, "{}{}", format_si_value(*r), "Ω"),
            ConstValue::Capacitance(c) => write!(f, "{}{}", format_si_value(*c), "F"),
            ConstValue::Inductance(l) => write!(f, "{}{}", format_si_value(*l), "H"),
            ConstValue::Power(w) => write!(f, "{}{}", format_si_value(*w), "W"),
            ConstValue::Frequency(hz) => write!(f, "{}{}", format_si_value(*hz), "Hz"),
            ConstValue::Time(s) => write!(f, "{}{}", format_si_value(*s), "s"),
        }
    }
}

/// Format a value with appropriate SI prefix (µ, m, k, M, G, etc.).
fn format_si_value(val: f64) -> std::string::String {
    let abs = val.abs();
    if abs == 0.0 {
        return "0".into();
    }
    let (scaled, prefix) = if abs >= 1e9 {
        (val / 1e9, "G")
    } else if abs >= 1e6 {
        (val / 1e6, "M")
    } else if abs >= 1e3 {
        (val / 1e3, "k")
    } else if abs >= 1.0 {
        (val, "")
    } else if abs >= 1e-3 {
        (val * 1e3, "m")
    } else if abs >= 1e-6 {
        (val * 1e6, "µ")
    } else if abs >= 1e-9 {
        (val * 1e9, "n")
    } else if abs >= 1e-12 {
        (val * 1e12, "p")
    } else {
        (val * 1e15, "f")
    };
    // Remove trailing zeros after decimal point
    let s = format!("{}", scaled);
    format!("{}{}", s, prefix)
}

// --- Unit parsing from token text ---

/// Parse a unit suffix string into (scale_factor, ConstValue constructor).
/// Returns None if the text is not a recognized unit.
///
/// The constructor is represented as a function pointer that wraps an f64
/// in the appropriate ConstValue variant.
pub fn parse_unit_suffix(text: &str) -> Option<(f64, fn(f64) -> ConstValue)> {
    match text {
        // Voltage
        "V" => Some((1.0, ConstValue::Voltage)),
        "mV" => Some((1e-3, ConstValue::Voltage)),
        "µV" | "uV" | "μV" => Some((1e-6, ConstValue::Voltage)),
        "kV" => Some((1e3, ConstValue::Voltage)),
        "Vdc" | "Vac" | "Vrms" | "Vpp" => Some((1.0, ConstValue::Voltage)),
        "nV" => Some((1e-9, ConstValue::Voltage)),

        // Current
        "A" => Some((1.0, ConstValue::Current)),
        "mA" => Some((1e-3, ConstValue::Current)),
        "µA" | "uA" | "μA" => Some((1e-6, ConstValue::Current)),
        "nA" => Some((1e-9, ConstValue::Current)),

        // Resistance
        "Ω" | "Ohm" | "ohm" => Some((1.0, ConstValue::Resistance)),
        "kΩ" | "kOhm" | "kohm" | "KOhm" => Some((1e3, ConstValue::Resistance)),
        "MΩ" | "MOhm" => Some((1e6, ConstValue::Resistance)),
        "mΩ" | "mOhm" | "mohm" | "milliOhm" => Some((1e-3, ConstValue::Resistance)),

        // Capacitance
        "F" => Some((1.0, ConstValue::Capacitance)),
        "mF" => Some((1e-3, ConstValue::Capacitance)),
        "µF" | "uF" | "μF" => Some((1e-6, ConstValue::Capacitance)),
        "nF" => Some((1e-9, ConstValue::Capacitance)),
        "pF" => Some((1e-12, ConstValue::Capacitance)),

        // Inductance
        "H" => Some((1.0, ConstValue::Inductance)),
        "mH" => Some((1e-3, ConstValue::Inductance)),
        "µH" | "uH" | "μH" => Some((1e-6, ConstValue::Inductance)),
        "nH" => Some((1e-9, ConstValue::Inductance)),

        // Frequency
        "Hz" => Some((1.0, ConstValue::Frequency)),
        "kHz" => Some((1e3, ConstValue::Frequency)),
        "MHz" => Some((1e6, ConstValue::Frequency)),
        "GHz" => Some((1e9, ConstValue::Frequency)),

        // Power
        "W" => Some((1.0, ConstValue::Power)),
        "mW" => Some((1e-3, ConstValue::Power)),
        "µW" | "uW" | "μW" => Some((1e-6, ConstValue::Power)),
        "nW" => Some((1e-9, ConstValue::Power)),

        // Time
        "s" => Some((1.0, ConstValue::Time)),
        "ms" => Some((1e-3, ConstValue::Time)),
        "µs" | "us" | "μs" => Some((1e-6, ConstValue::Time)),
        "ns" => Some((1e-9, ConstValue::Time)),
        "ps" => Some((1e-12, ConstValue::Time)),

        _ => None,
    }
}

/// Parse a standalone SI prefix (single letter without base unit).
/// Returns the scale factor, or None if not a recognized prefix.
pub fn parse_si_prefix(text: &str) -> Option<f64> {
    match text {
        "G" => Some(1e9),
        "M" => Some(1e6),
        "k" | "K" => Some(1e3),
        "m" => Some(1e-3),
        "u" => Some(1e-6),
        "n" => Some(1e-9),
        "p" => Some(1e-12),
        "f" => Some(1e-15),
        _ => None,
    }
}

// --- Built-in functions for board design ---

/// Compute parallel combination of two resistances: 1/(1/r1 + 1/r2).
pub fn builtin_parallel(args: &[ConstValue]) -> Result<ConstValue, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(format!(
            "parallel() expects 2 arguments, got {}",
            args.len()
        )));
    }
    match (&args[0], &args[1]) {
        (ConstValue::Resistance(r1), ConstValue::Resistance(r2)) => {
            if *r1 == 0.0 || *r2 == 0.0 {
                return Err(EvalError::DivisionByZero);
            }
            Ok(ConstValue::Resistance(1.0 / (1.0 / r1 + 1.0 / r2)))
        }
        _ => Err(EvalError::TypeMismatch(
            "parallel() expects two resistance values".into(),
        )),
    }
}

/// Compute voltage divider ratio: r_low / (r_high + r_low).
pub fn builtin_divider_ratio(args: &[ConstValue]) -> Result<ConstValue, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(format!(
            "divider_ratio() expects 2 arguments, got {}",
            args.len()
        )));
    }
    match (&args[0], &args[1]) {
        (ConstValue::Resistance(r_high), ConstValue::Resistance(r_low)) => {
            let sum = r_high + r_low;
            if sum == 0.0 {
                return Err(EvalError::DivisionByZero);
            }
            Ok(ConstValue::Float(r_low / sum))
        }
        _ => Err(EvalError::TypeMismatch(
            "divider_ratio() expects two resistance values".into(),
        )),
    }
}

/// Compute RC low-pass cutoff frequency: 1/(2π·R·C).
pub fn builtin_rc_cutoff(args: &[ConstValue]) -> Result<ConstValue, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(format!(
            "rc_cutoff() expects 2 arguments, got {}",
            args.len()
        )));
    }
    match (&args[0], &args[1]) {
        (ConstValue::Resistance(r), ConstValue::Capacitance(c)) => {
            let rc = r * c;
            if rc == 0.0 {
                return Err(EvalError::DivisionByZero);
            }
            Ok(ConstValue::Frequency(1.0 / (2.0 * std::f64::consts::PI * rc)))
        }
        _ => Err(EvalError::TypeMismatch(
            "rc_cutoff() expects (resistance, capacitance)".into(),
        )),
    }
}

/// Compute LC resonant frequency: 1/(2π·√(L·C)).
pub fn builtin_lc_resonance(args: &[ConstValue]) -> Result<ConstValue, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(format!(
            "lc_resonance() expects 2 arguments, got {}",
            args.len()
        )));
    }
    match (&args[0], &args[1]) {
        (ConstValue::Inductance(l), ConstValue::Capacitance(c)) => {
            let lc = l * c;
            if lc <= 0.0 {
                return Err(EvalError::TypeMismatch(
                    "lc_resonance() requires positive L and C".into(),
                ));
            }
            Ok(ConstValue::Frequency(
                1.0 / (2.0 * std::f64::consts::PI * lc.sqrt()),
            ))
        }
        _ => Err(EvalError::TypeMismatch(
            "lc_resonance() expects (inductance, capacitance)".into(),
        )),
    }
}

/// Compute power dissipation: V * I.
pub fn builtin_power_dissipation(args: &[ConstValue]) -> Result<ConstValue, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(format!(
            "power_dissipation() expects 2 arguments, got {}",
            args.len()
        )));
    }
    match (&args[0], &args[1]) {
        (ConstValue::Voltage(v), ConstValue::Current(i)) => Ok(ConstValue::Power(v * i)),
        _ => Err(EvalError::TypeMismatch(
            "power_dissipation() expects (voltage, current)".into(),
        )),
    }
}

/// Compute thermal rise: power * θ_JA.
pub fn builtin_thermal_rise(args: &[ConstValue]) -> Result<ConstValue, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(format!(
            "thermal_rise() expects 2 arguments, got {}",
            args.len()
        )));
    }
    match (&args[0], &args[1]) {
        (ConstValue::Power(p), ConstValue::Float(theta_ja)) => {
            Ok(ConstValue::Float(p * theta_ja))
        }
        _ => Err(EvalError::TypeMismatch(
            "thermal_rise() expects (power, float)".into(),
        )),
    }
}

/// Snap resistance to nearest E96 standard value.
pub fn builtin_nearest_e96(args: &[ConstValue]) -> Result<ConstValue, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(format!(
            "nearest_e96() expects 1 argument, got {}",
            args.len()
        )));
    }
    match &args[0] {
        ConstValue::Resistance(r) => Ok(ConstValue::Resistance(snap_to_e_series(*r, &E96_VALUES))),
        _ => Err(EvalError::TypeMismatch(
            "nearest_e96() expects a resistance value".into(),
        )),
    }
}

/// Snap resistance to nearest E24 standard value.
pub fn builtin_nearest_e24(args: &[ConstValue]) -> Result<ConstValue, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(format!(
            "nearest_e24() expects 1 argument, got {}",
            args.len()
        )));
    }
    match &args[0] {
        ConstValue::Resistance(r) => Ok(ConstValue::Resistance(snap_to_e_series(*r, &E24_VALUES))),
        _ => Err(EvalError::TypeMismatch(
            "nearest_e24() expects a resistance value".into(),
        )),
    }
}

/// Snap a value to the nearest E-series standard value.
fn snap_to_e_series(value: f64, series: &[f64]) -> f64 {
    if value <= 0.0 {
        return series[0];
    }
    // Normalize to [1.0, 10.0) range
    let log10 = value.log10();
    let decade = log10.floor();
    let normalized = value / 10f64.powf(decade);

    // Find the closest series value
    let mut best = series[0];
    let mut best_ratio = (normalized / best - 1.0).abs();
    for &sv in &series[1..] {
        let ratio = (normalized / sv - 1.0).abs();
        if ratio < best_ratio {
            best = sv;
            best_ratio = ratio;
        }
    }
    best * 10f64.powf(decade)
}

// E24 standard values (normalized to 1.0-10.0 range)
const E24_VALUES: [f64; 24] = [
    1.0, 1.1, 1.2, 1.3, 1.5, 1.6, 1.8, 2.0, 2.2, 2.4, 2.7, 3.0, 3.3, 3.6, 3.9, 4.3, 4.7, 5.1,
    5.6, 6.2, 6.8, 7.5, 8.2, 9.1,
];

// E96 standard values (normalized to 1.0-10.0 range)
const E96_VALUES: [f64; 96] = [
    1.00, 1.02, 1.05, 1.07, 1.10, 1.13, 1.15, 1.18, 1.21, 1.24, 1.27, 1.30, 1.33, 1.37, 1.40,
    1.43, 1.47, 1.50, 1.54, 1.58, 1.62, 1.65, 1.69, 1.74, 1.78, 1.82, 1.87, 1.91, 1.96, 2.00,
    2.05, 2.10, 2.15, 2.21, 2.26, 2.32, 2.37, 2.43, 2.49, 2.55, 2.61, 2.67, 2.74, 2.80, 2.87,
    2.94, 3.01, 3.09, 3.16, 3.24, 3.32, 3.40, 3.48, 3.57, 3.65, 3.74, 3.83, 3.92, 4.02, 4.12,
    4.22, 4.32, 4.42, 4.53, 4.64, 4.75, 4.87, 4.99, 5.11, 5.23, 5.36, 5.49, 5.62, 5.76, 5.90,
    6.04, 6.19, 6.34, 6.49, 6.65, 6.81, 6.98, 7.15, 7.32, 7.50, 7.68, 7.87, 8.06, 8.25, 8.45,
    8.66, 8.87, 9.09, 9.31, 9.53, 9.76,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_arithmetic() {
        let a = ConstValue::Integer(10);
        let b = ConstValue::Integer(3);
        assert_eq!(a.clone().add(b.clone()).unwrap(), ConstValue::Integer(13));
        assert_eq!(a.clone().sub(b.clone()).unwrap(), ConstValue::Integer(7));
        assert_eq!(a.clone().mul(b.clone()).unwrap(), ConstValue::Integer(30));
        assert_eq!(a.div(b).unwrap(), ConstValue::Integer(3));
    }

    #[test]
    fn test_voltage_arithmetic() {
        let a = ConstValue::Voltage(3.3);
        let b = ConstValue::Voltage(1.8);
        match a.clone().add(b.clone()).unwrap() {
            ConstValue::Voltage(v) => assert!((v - 5.1).abs() < 1e-10),
            other => panic!("expected Voltage, got {:?}", other),
        }
        match a.sub(b).unwrap() {
            ConstValue::Voltage(v) => assert!((v - 1.5).abs() < 1e-10),
            other => panic!("expected Voltage, got {:?}", other),
        }
    }

    #[test]
    fn test_ohms_law() {
        let v = ConstValue::Voltage(5.0);
        let r = ConstValue::Resistance(1000.0);
        // V / R = I
        let i = v.clone().div(r.clone()).unwrap();
        assert_eq!(i, ConstValue::Current(0.005));
        // V * I = P
        let p = v.mul(ConstValue::Current(0.005)).unwrap();
        assert_eq!(p, ConstValue::Power(0.025));
    }

    #[test]
    fn test_dimension_mismatch() {
        let v = ConstValue::Voltage(3.3);
        let a = ConstValue::Current(0.1);
        assert!(v.add(a).is_err());
    }

    #[test]
    fn test_division_by_zero() {
        let a = ConstValue::Integer(10);
        let b = ConstValue::Integer(0);
        assert!(a.div(b).is_err());
    }

    #[test]
    fn test_parse_unit_suffix() {
        let (scale, ctor) = parse_unit_suffix("kΩ").unwrap();
        assert_eq!(scale, 1e3);
        assert_eq!(ctor(4700.0), ConstValue::Resistance(4700.0));

        let (scale, ctor) = parse_unit_suffix("mA").unwrap();
        assert_eq!(scale, 1e-3);
        assert_eq!(ctor(0.1), ConstValue::Current(0.1));

        assert!(parse_unit_suffix("foobar").is_none());
    }

    #[test]
    fn test_display_si_prefix() {
        assert_eq!(format!("{}", ConstValue::Resistance(4700.0)), "4.7kΩ");
        assert_eq!(format!("{}", ConstValue::Current(0.001)), "1mA");
        assert_eq!(format!("{}", ConstValue::Capacitance(0.0000001)), "100nF");
        assert_eq!(format!("{}", ConstValue::Voltage(3.3)), "3.3V");
    }

    #[test]
    fn test_scalar_times_physical() {
        let n = ConstValue::Integer(2);
        let r = ConstValue::Resistance(1000.0);
        assert_eq!(n.mul(r).unwrap(), ConstValue::Resistance(2000.0));
    }

    #[test]
    fn test_parallel_resistance() {
        let args = vec![
            ConstValue::Resistance(1000.0),
            ConstValue::Resistance(1000.0),
        ];
        assert_eq!(builtin_parallel(&args).unwrap(), ConstValue::Resistance(500.0));
    }

    #[test]
    fn test_divider_ratio() {
        let args = vec![
            ConstValue::Resistance(10000.0), // R_high
            ConstValue::Resistance(10000.0), // R_low
        ];
        assert_eq!(builtin_divider_ratio(&args).unwrap(), ConstValue::Float(0.5));
    }

    #[test]
    fn test_nearest_e24() {
        // 4800Ω should snap to 4700Ω (E24 value)
        let args = vec![ConstValue::Resistance(4800.0)];
        let result = builtin_nearest_e24(&args).unwrap();
        if let ConstValue::Resistance(r) = result {
            assert!((r - 4700.0).abs() < 1.0);
        } else {
            panic!("expected Resistance");
        }
    }

    #[test]
    fn test_negate() {
        assert_eq!(
            ConstValue::Integer(5).negate().unwrap(),
            ConstValue::Integer(-5)
        );
        assert_eq!(
            ConstValue::Voltage(3.3).negate().unwrap(),
            ConstValue::Voltage(-3.3)
        );
    }

    // --- Dimensional analysis tests (RFC Section 10) ---

    #[test]
    fn test_dimension_voltage_times_current_equals_power() {
        // 3.3V * 100mA = 0.33W
        let v = ConstValue::Voltage(3.3);
        let i = ConstValue::Current(0.1);
        let p = v.mul(i).unwrap();
        match p {
            ConstValue::Power(w) => assert!((w - 0.33).abs() < 1e-10),
            other => panic!("expected Power, got {:?}", other),
        }
    }

    #[test]
    fn test_dimension_voltage_div_current_equals_resistance() {
        // 3.3V / 100mA = 33Ω
        let v = ConstValue::Voltage(3.3);
        let i = ConstValue::Current(0.1);
        let r = v.div(i).unwrap();
        match r {
            ConstValue::Resistance(ohm) => assert!((ohm - 33.0).abs() < 1e-10),
            other => panic!("expected Resistance, got {:?}", other),
        }
    }

    #[test]
    fn test_dimension_voltage_plus_current_is_error() {
        // 3.3V + 100mA = ERROR
        let v = ConstValue::Voltage(3.3);
        let i = ConstValue::Current(0.1);
        assert!(v.add(i).is_err());
    }

    #[test]
    fn test_dimension_resistance_times_capacitance_equals_time() {
        // R * C = time constant (τ)
        // 10kΩ * 100nF = 1ms
        let r = ConstValue::Resistance(10_000.0);
        let c = ConstValue::Capacitance(100e-9);
        let tau = r.mul(c).unwrap();
        match tau {
            ConstValue::Time(s) => assert!((s - 1e-3).abs() < 1e-12),
            other => panic!("expected Time, got {:?}", other),
        }
    }

    #[test]
    fn test_dimension_one_over_time_equals_frequency() {
        // 1 / τ = frequency
        let tau = ConstValue::Time(1e-3); // 1ms
        let f = ConstValue::Float(1.0).div(tau).unwrap();
        match f {
            ConstValue::Frequency(hz) => assert!((hz - 1000.0).abs() < 1e-6),
            other => panic!("expected Frequency, got {:?}", other),
        }
    }

    #[test]
    fn test_dimension_inductance_times_capacitance_equals_time_squared() {
        // L * C produces dimension [V^0, A^0, s^2] — not a standard unit
        // The system should report an error for unrecognized dimensions
        let l = ConstValue::Inductance(1e-3); // 1mH
        let c = ConstValue::Capacitance(1e-6); // 1µF
        let result = l.mul(c);
        // L*C = s^2, which is not a recognized dimension
        assert!(result.is_err());
    }

    #[test]
    fn test_dimension_voltage_div_resistance_equals_current() {
        // V / R = I
        let v = ConstValue::Voltage(5.0);
        let r = ConstValue::Resistance(1000.0);
        let i = v.div(r).unwrap();
        match i {
            ConstValue::Current(a) => assert!((a - 0.005).abs() < 1e-12),
            other => panic!("expected Current, got {:?}", other),
        }
    }

    #[test]
    fn test_dimension_same_dimension_ratio() {
        // V / V = dimensionless (Float)
        let v1 = ConstValue::Voltage(3.3);
        let v2 = ConstValue::Voltage(5.0);
        let ratio = v1.div(v2).unwrap();
        match ratio {
            ConstValue::Float(f) => assert!((f - 0.66).abs() < 0.01),
            other => panic!("expected Float ratio, got {:?}", other),
        }
    }

    #[test]
    fn test_dimension_power_div_voltage_equals_current() {
        // P / V = I
        let p = ConstValue::Power(1.0); // 1W
        let v = ConstValue::Voltage(5.0);
        let i = p.div(v).unwrap();
        match i {
            ConstValue::Current(a) => assert!((a - 0.2).abs() < 1e-10),
            other => panic!("expected Current, got {:?}", other),
        }
    }

    #[test]
    fn test_dimension_struct_display() {
        assert_eq!(format!("{}", Dimension::VOLTAGE), "voltage");
        assert_eq!(format!("{}", Dimension::DIMENSIONLESS), "dimensionless");
        // L*C = s^2, not a standard dimension
        let s_squared = Dimension::INDUCTANCE.mul(Dimension::CAPACITANCE);
        assert_eq!(format!("{}", s_squared), "[s^2]");
    }
}
