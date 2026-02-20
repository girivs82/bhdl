//! Safety annotation types for ISO 26262 / IEC 61508 compliance.
//!
//! Provides data structures for safety goals, safety mechanisms,
//! fault injection tests, and component derating annotations.

use std::collections::HashMap;

/// ASIL (Automotive Safety Integrity Level) per ISO 26262.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AsilLevel {
    /// Quality Management (no safety requirement)
    QM,
    /// ASIL A (lowest safety level)
    A,
    /// ASIL B
    B,
    /// ASIL C
    C,
    /// ASIL D (highest safety level)
    D,
}

impl AsilLevel {
    /// Parse from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "QM" => Some(AsilLevel::QM),
            "A" | "ASIL_A" => Some(AsilLevel::A),
            "B" | "ASIL_B" => Some(AsilLevel::B),
            "C" | "ASIL_C" => Some(AsilLevel::C),
            "D" | "ASIL_D" => Some(AsilLevel::D),
            _ => None,
        }
    }

    /// ASIL ordering: QM < A < B < C < D
    pub fn level(&self) -> u8 {
        match self {
            AsilLevel::QM => 0,
            AsilLevel::A => 1,
            AsilLevel::B => 2,
            AsilLevel::C => 3,
            AsilLevel::D => 4,
        }
    }
}

/// SIL (Safety Integrity Level) per IEC 61508.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SilLevel {
    SIL1,
    SIL2,
    SIL3,
    SIL4,
}

impl SilLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "SIL1" | "1" => Some(SilLevel::SIL1),
            "SIL2" | "2" => Some(SilLevel::SIL2),
            "SIL3" | "3" => Some(SilLevel::SIL3),
            "SIL4" | "4" => Some(SilLevel::SIL4),
            _ => None,
        }
    }
}

/// A safety goal declaration.
///
/// ```bhdl
/// safety_goal SG_OVP {
///     id: "SG-001";
///     title: "Prevent output overvoltage";
///     asil: B;
///     ftti: 10ms;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SafetyGoal {
    /// Internal name (e.g., "SG_OVP")
    pub name: String,
    /// Formal ID (e.g., "SG-001")
    pub id: Option<String>,
    /// Human-readable title
    pub title: Option<String>,
    /// Required ASIL level
    pub asil: Option<AsilLevel>,
    /// Fault Tolerant Time Interval in seconds
    pub ftti_s: Option<f64>,
    /// Optional description
    pub description: Option<String>,
    /// Additional properties
    pub properties: HashMap<String, String>,
}

/// A safety mechanism annotation.
///
/// ```bhdl
/// #[safety_mechanism(type: ovp_monitor, dc: 99%, implements: SG_OVP)]
/// ```
#[derive(Debug, Clone)]
pub struct SafetyMechanism {
    /// Mechanism type (e.g., "ovp_monitor", "watchdog", "voting")
    pub mechanism_type: String,
    /// Diagnostic Coverage (0.0 - 1.0)
    pub diagnostic_coverage: Option<f64>,
    /// Latent Coverage (0.0 - 1.0)
    pub latent_coverage: Option<f64>,
    /// Detection mode
    pub detection_mode: DetectionMode,
    /// Response time in microseconds
    pub response_time_us: Option<f64>,
    /// Safety goal IDs this mechanism implements
    pub implements: Vec<String>,
}

/// How a safety mechanism detects faults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionMode {
    /// Continuous monitoring
    Continuous,
    /// Checked at boot
    Boot,
    /// Periodic check
    Periodic,
    /// Checked on demand
    OnDemand,
}

impl DetectionMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "continuous" => Some(DetectionMode::Continuous),
            "boot" => Some(DetectionMode::Boot),
            "periodic" => Some(DetectionMode::Periodic),
            "on_demand" | "ondemand" => Some(DetectionMode::OnDemand),
            _ => None,
        }
    }
}

/// A fault injection test definition.
///
/// ```bhdl
/// fault_inject short(reg.VOUT, VIN) -> verify {
///     assert comparator.OUT == low within 100us;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct FaultInjection {
    /// Type of fault to inject
    pub fault_type: FaultType,
    /// Assertions to verify after fault injection
    pub assertions: Vec<SafetyAssertion>,
}

/// Types of faults that can be injected.
#[derive(Debug, Clone)]
pub enum FaultType {
    /// Short circuit between two nodes
    Short(String, String),
    /// Open circuit (break a connection)
    Open(String),
    /// Value drift by a percentage or absolute amount
    Drift(String, f64),
    /// Stuck at a specific value
    StuckAt(String, String),
}

/// An assertion in a fault injection test.
#[derive(Debug, Clone)]
pub struct SafetyAssertion {
    /// The expression being asserted
    pub expression: String,
    /// Optional timing constraint (within X time)
    pub within_us: Option<f64>,
}

/// Component derating annotations.
///
/// ```bhdl
/// #[safety(derating: voltage=80%, current=70%, temperature=85C_max)]
/// ```
#[derive(Debug, Clone)]
pub struct DeratingAnnotation {
    /// Voltage derating factor (0.0 - 1.0)
    pub voltage: Option<f64>,
    /// Current derating factor (0.0 - 1.0)
    pub current: Option<f64>,
    /// Maximum temperature in Celsius
    pub max_temperature_c: Option<f64>,
    /// Power derating factor
    pub power: Option<f64>,
}

/// Redundancy annotation for safety-critical components.
///
/// ```bhdl
/// #[safety(redundant, voting: 2_of_3)]
/// ```
#[derive(Debug, Clone)]
pub struct RedundancyAnnotation {
    /// Voting scheme (e.g., "2_of_3", "1_of_2")
    pub voting: Option<VotingScheme>,
    /// Whether this is hot standby or cold standby
    pub standby_mode: Option<StandbyMode>,
}

/// Voting scheme for redundant components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VotingScheme {
    /// 1-out-of-2 (any one must agree)
    OneOfTwo,
    /// 2-out-of-3 (majority voting)
    TwoOfThree,
    /// 2-out-of-4
    TwoOfFour,
    /// Custom M-of-N
    MOfN(u32, u32),
}

impl VotingScheme {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "1_of_2" | "1of2" => Some(VotingScheme::OneOfTwo),
            "2_of_3" | "2of3" => Some(VotingScheme::TwoOfThree),
            "2_of_4" | "2of4" => Some(VotingScheme::TwoOfFour),
            _ => None,
        }
    }
}

/// Standby mode for redundant components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandbyMode {
    /// Both active simultaneously
    Hot,
    /// Backup activated on failure
    Cold,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asil_level_parsing() {
        assert_eq!(AsilLevel::from_str("QM"), Some(AsilLevel::QM));
        assert_eq!(AsilLevel::from_str("A"), Some(AsilLevel::A));
        assert_eq!(AsilLevel::from_str("B"), Some(AsilLevel::B));
        assert_eq!(AsilLevel::from_str("C"), Some(AsilLevel::C));
        assert_eq!(AsilLevel::from_str("D"), Some(AsilLevel::D));
        assert_eq!(AsilLevel::from_str("ASIL_B"), Some(AsilLevel::B));
        assert_eq!(AsilLevel::from_str("X"), None);
    }

    #[test]
    fn test_asil_ordering() {
        assert!(AsilLevel::QM.level() < AsilLevel::A.level());
        assert!(AsilLevel::A.level() < AsilLevel::B.level());
        assert!(AsilLevel::B.level() < AsilLevel::C.level());
        assert!(AsilLevel::C.level() < AsilLevel::D.level());
    }

    #[test]
    fn test_sil_level_parsing() {
        assert_eq!(SilLevel::from_str("SIL1"), Some(SilLevel::SIL1));
        assert_eq!(SilLevel::from_str("SIL4"), Some(SilLevel::SIL4));
        assert_eq!(SilLevel::from_str("1"), Some(SilLevel::SIL1));
        assert_eq!(SilLevel::from_str("invalid"), None);
    }

    #[test]
    fn test_detection_mode_parsing() {
        assert_eq!(DetectionMode::from_str("continuous"), Some(DetectionMode::Continuous));
        assert_eq!(DetectionMode::from_str("boot"), Some(DetectionMode::Boot));
        assert_eq!(DetectionMode::from_str("periodic"), Some(DetectionMode::Periodic));
        assert_eq!(DetectionMode::from_str("on_demand"), Some(DetectionMode::OnDemand));
    }

    #[test]
    fn test_voting_scheme_parsing() {
        assert_eq!(VotingScheme::from_str("2_of_3"), Some(VotingScheme::TwoOfThree));
        assert_eq!(VotingScheme::from_str("1_of_2"), Some(VotingScheme::OneOfTwo));
        assert_eq!(VotingScheme::from_str("2_of_4"), Some(VotingScheme::TwoOfFour));
    }

    #[test]
    fn test_safety_goal_creation() {
        let goal = SafetyGoal {
            name: "SG_OVP".to_string(),
            id: Some("SG-001".to_string()),
            title: Some("Prevent output overvoltage".to_string()),
            asil: Some(AsilLevel::B),
            ftti_s: Some(0.01), // 10ms
            description: None,
            properties: HashMap::new(),
        };
        assert_eq!(goal.asil.unwrap().level(), 2);
    }

    #[test]
    fn test_safety_mechanism_creation() {
        let mechanism = SafetyMechanism {
            mechanism_type: "ovp_monitor".to_string(),
            diagnostic_coverage: Some(0.99),
            latent_coverage: None,
            detection_mode: DetectionMode::Continuous,
            response_time_us: Some(100.0),
            implements: vec!["SG-001".to_string()],
        };
        assert_eq!(mechanism.diagnostic_coverage.unwrap(), 0.99);
    }

    #[test]
    fn test_fault_injection_creation() {
        let injection = FaultInjection {
            fault_type: FaultType::Short("VOUT".to_string(), "VIN".to_string()),
            assertions: vec![
                SafetyAssertion {
                    expression: "comparator.OUT == low".to_string(),
                    within_us: Some(100.0),
                },
            ],
        };
        match &injection.fault_type {
            FaultType::Short(a, b) => {
                assert_eq!(a, "VOUT");
                assert_eq!(b, "VIN");
            }
            _ => panic!("Expected Short fault type"),
        }
    }
}
