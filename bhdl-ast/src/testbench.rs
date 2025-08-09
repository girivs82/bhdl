//! AST nodes for BHDL testbench definitions

use crate::{SyntaxKind, SyntaxNode, SyntaxToken, AstNode, BhdlLanguage, HasName};
use rowan::ast::support;

/// A testbench definition: `testbench Name for Board { ... }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TestbenchDef {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for TestbenchDef {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TESTBENCH_DEF
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl HasName for TestbenchDef {}

impl TestbenchDef {
    /// Get the target board name
    pub fn target_board(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        // Skip past 'testbench', name, and 'for' to get the board name
        let mut found_for = false;
        for token in self.syntax.children_with_tokens().filter_map(|e| e.into_token()) {
            if token.kind() == SyntaxKind::FOR_KW {
                found_for = true;
            } else if found_for && token.kind() == SyntaxKind::IDENT {
                return Some(token);
            }
        }
        None
    }

    pub fn simulation_block(&self) -> Option<SimulationBlock> {
        support::child(&self.syntax)
    }

    pub fn scopes(&self) -> impl Iterator<Item = ScopeDef> {
        support::children(&self.syntax)
    }

    pub fn stimulus_block(&self) -> Option<StimulusBlock> {
        support::child(&self.syntax)
    }

    pub fn verify_block(&self) -> Option<VerifyBlock> {
        support::child(&self.syntax)
    }

    pub fn measure_block(&self) -> Option<MeasureBlock> {
        support::child(&self.syntax)
    }
}

/// Simulation configuration block
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimulationBlock {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for SimulationBlock {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SIMULATION_BLOCK
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl SimulationBlock {
    pub fn duration(&self) -> Option<TimeSpec> {
        self.find_config_value("duration")
    }

    pub fn timestep(&self) -> Option<TimeSpec> {
        self.find_config_value("timestep")
    }

    pub fn temperature(&self) -> Option<SyntaxNode<BhdlLanguage>> {
        self.find_config_value_raw("temperature")
    }

    pub fn solver(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.find_config_token("solver")
    }

    fn find_config_value<T: AstNode<Language = BhdlLanguage>>(&self, key: &str) -> Option<T> {
        self.find_config_pattern(key, |node| T::cast(node))
    }

    fn find_config_value_raw(&self, key: &str) -> Option<SyntaxNode<BhdlLanguage>> {
        self.find_config_pattern(key, Some)
    }

    fn find_config_token(&self, key: &str) -> Option<SyntaxToken<BhdlLanguage>> {
        // Find key: value pattern and return the value token
        let mut found_key = false;
        let mut passed_colon = false;
        
        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if !found_key && token.kind() == SyntaxKind::IDENT && token.text() == key {
                        found_key = true;
                    } else if found_key && !passed_colon && token.kind() == SyntaxKind::COLON {
                        passed_colon = true;
                    } else if passed_colon && token.kind() == SyntaxKind::IDENT {
                        return Some(token);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn find_config_pattern<T>(
        &self, 
        key: &str, 
        f: impl Fn(SyntaxNode<BhdlLanguage>) -> Option<T>
    ) -> Option<T> {
        // Find pattern: IDENT(key) COLON NODE
        let mut found_key = false;
        let mut passed_colon = false;
        
        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if !found_key && token.kind() == SyntaxKind::IDENT && token.text() == key {
                        found_key = true;
                    } else if found_key && !passed_colon && token.kind() == SyntaxKind::COLON {
                        passed_colon = true;
                    }
                }
                rowan::NodeOrToken::Node(node) => {
                    if passed_colon {
                        return f(node);
                    }
                }
            }
        }
        None
    }
}

/// Scope definition for waveform capture
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopeDef {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for ScopeDef {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SCOPE_DEF
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl ScopeDef {
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::STRING)
    }

    pub fn signals(&self) -> impl Iterator<Item = SignalRef> {
        // Find signals: keyword, then collect signal refs until semicolon
        let mut in_signals = false;
        let mut refs = Vec::new();
        
        for child in self.syntax.children() {
            if child.kind() == SyntaxKind::SIGNALS_KW {
                in_signals = true;
            } else if in_signals && child.kind() == SyntaxKind::NET_REF {
                if let Some(signal_ref) = SignalRef::cast(child) {
                    refs.push(signal_ref);
                }
            }
        }
        
        refs.into_iter()
    }

    pub fn capture_mode(&self) -> Option<CaptureMode> {
        support::child(&self.syntax)
    }
}

/// Signal reference in scopes (@VIN, U1.FB, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignalRef {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for SignalRef {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::NET_REF
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

/// Capture mode configuration
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaptureMode {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for CaptureMode {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CAPTURE_MODE
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

/// Stimulus block
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StimulusBlock {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for StimulusBlock {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::STIMULUS_BLOCK
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl StimulusBlock {
    pub fn assignments(&self) -> impl Iterator<Item = StimulusAssign> {
        support::children(&self.syntax)
    }
}

/// Stimulus assignment
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StimulusAssign {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for StimulusAssign {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::STIMULUS_ASSIGN
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl StimulusAssign {
    pub fn signal(&self) -> Option<SignalRef> {
        support::child(&self.syntax)
    }

    pub fn waveform(&self) -> Option<WaveformExpr> {
        support::child(&self.syntax)
    }
}

/// Waveform expression
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WaveformExpr {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for WaveformExpr {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::WAVEFORM_EXPR
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

/// Verify block
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VerifyBlock {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for VerifyBlock {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::VERIFY_BLOCK
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl VerifyBlock {
    pub fn assertions(&self) -> impl Iterator<Item = Assertion> {
        support::children(&self.syntax)
    }
}

/// Assertion
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Assertion {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for Assertion {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ASSERTION
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl Assertion {
    pub fn message(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::STRING)
    }

    pub fn after_time(&self) -> Option<TimeSpec> {
        // Find AFTER_KW followed by TIME_SPEC
        let mut found_after = false;
        for child in self.syntax.children() {
            if child.kind() == SyntaxKind::AFTER_KW {
                found_after = true;
            } else if found_after && child.kind() == SyntaxKind::TIME_SPEC {
                return TimeSpec::cast(child);
            }
        }
        None
    }
}

/// Measure block
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeasureBlock {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for MeasureBlock {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MEASURE_BLOCK
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl MeasureBlock {
    pub fn measurements(&self) -> impl Iterator<Item = Measurement> {
        support::children(&self.syntax)
    }
}

/// Measurement
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Measurement {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for Measurement {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MEASUREMENT
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl HasName for Measurement {}

/// Time specification (10ms, 1us, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimeSpec {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for TimeSpec {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TIME_SPEC
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl TimeSpec {
    pub fn number(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::NUMBER)
    }

    pub fn unit(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| matches!(t.kind(), 
                SyntaxKind::MS_UNIT | SyntaxKind::US_UNIT | 
                SyntaxKind::NS_UNIT | SyntaxKind::S_UNIT | 
                SyntaxKind::UNIT_IDENTIFIER
            ))
    }
}