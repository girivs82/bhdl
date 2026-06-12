//! Abstract Syntax Tree (AST) for the BHDL language.

// Re-export core parser types
pub use bhdl_parser::{SyntaxKind, BhdlLanguage}; // Keep base language and kind

// Use rowan types directly for Node/Token
pub use rowan::{SyntaxNode, SyntaxToken};

// AST node trait (re-exported)
pub use rowan::ast::AstNode;

// Module declarations
pub mod blocks;
pub mod items;
pub mod common;
pub mod expr; 
pub mod source_file; // Ensure source_file module is declared
pub mod flow; // New module for circuit flow paradigm AST nodes
pub mod visitor; // Visitor pattern for AST traversal
pub mod pretty_print; // Pretty-printing functionality
pub mod validation; // AST validation and semantic analysis
pub mod transform; // AST transformation utilities
pub mod symbol_table; // Symbol table management for semantic analysis
pub mod semantic_analysis; // Semantic analysis and type checking
pub mod constraint_resolver; // Constraint resolution and validation
pub mod v2_statements; // v2.0 statement AST nodes
pub mod v2_board; // v2.0 board extensions
pub mod hierarchical; // Hierarchical entity support
pub mod interfaces; // Interface-specific AST nodes
pub mod attributes; // Behavioral modeling attribute support
pub mod behavioral; // When blocks and behavioral statements
pub mod behavioral_models; // Behavioral model and optimization annotations
pub mod testbench; // Testbench AST nodes
pub mod safety; // Safety-related AST nodes for ISO 26262 compliance
pub mod safety_hierarchy; // Hierarchical safety requirements
pub mod enums; // Enum types and match expressions
pub mod traits; // Trait definitions and implementations

// Core HasName trait (defined here)
pub trait HasName: AstNode<Language = BhdlLanguage> {
    /// Returns the name token associated with this node.
    fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
}

// Consolidated Re-exports
pub use source_file::{SourceFile, Item};
pub use items::{Board, Entity, ComponentDef, InterfaceDef, TypedefDef, TypedefBase, ParamList, EntityParam, ImportStmt, ConstDecl, PowerDomain, SourcesBlock, SourceDefinition, DistributionBlock, DistributionPinList, DecouplingBlock, DecouplingRule, CapSpec, GenericParams, GenericParamNode, WhereClause, TypeArgs, ExpansionBlock, SymbolDef, SymbolSide, SymbolGroup, SymbolPart, LayoutDef, PartFamilyDef, ClassPattern, RequireClause};
pub use blocks::{LayerStackupBlock, DefaultDesignRulesBlock, ConstrainBlock, GenerateBlock, ForLoopGenerate, IfGenerate, LayerDef};
pub use common::{Ident, ParamAssign, PortDecl, PinDecl, NetDecl, TypeRef, BusSuffix, RangeExpr, Value, ComponentInst, PinRef, NetRef, IdentRef, SimpleIdentRef, ComponentType, PortDirection, ParamDecl, ParamAssignBlock, ParamPlaceholder, PinMetadata, MetadataPair};
pub use expr::{Expr, PrefixExpr, BinaryExpr, TernaryExpr, FunctionCallExpr, ArgumentList, FlowExpr as ExprFlowExpr, ComponentInstExpr, ArrayExpr, StructLiteral, StructField};
pub use flow::{FlowStmt, FlowExpr, FlowElement, ComponentInstantiation, GenerateStmt, ConditionalStmt, ConditionalExpr, AssignStmt};
pub use hierarchical::{EntityInst, PortMapping, PortPinRef, ConnectionTarget, ScopedAttribute, AttributePath};
pub use visitor::{AstVisitor, ConstructCounter, ComponentTypeCollector};
pub use attributes::{AttributeDecl, AttributeType, AttributeDependency};
pub use pretty_print::{PrettyPrint, PrettyPrintConfig, PrettyPrintContext};
pub use validation::{ValidationError, ValidationReport, ValidationContext, Validator, ParameterDef, VariableInfo, validate_board, validate_expression, is_valid_board};
pub use transform::{TransformResult, TransformError, Transformer, TransformContext, CompositeTransformer, create_default_transform_pipeline, transform_board, apply_variable_substitutions, unroll_generate_statements, flatten_flow_expressions};
pub use symbol_table::{SymbolTable, Symbol, SymbolKind, SymbolError, Scope, ScopeKind, ScopeId, SourceLocation, SymbolTableBuilder, build_symbol_table, validate_symbol_references};
pub use semantic_analysis::{SemanticAnalyzer, SemanticContext, SemanticError, BhdlType, UnitType, ComponentTypeInfo, ParameterInfo, PinInfo, PinDirection, InterfaceInfo, analyze_board_semantics, is_semantically_valid, get_expression_type};
pub use enums::{EnumDef, EnumVariant, MatchExpr, MatchArm, MatchPattern, PatternKind};
pub use traits::{TraitDef, TraitImpl, TraitPin, TraitConst};
pub use constraint_resolver::{ConstraintResolver, Constraint, ConstraintType, ConstraintRule, ConstraintSeverity, ConstraintContext, ConstraintViolation, ConstraintResult, ComparisonOp, resolve_board_constraints, board_satisfies_constraints, is_standard_resistor_value};
pub use v2_statements::{Statement, PowerDecl, GroundDecl, ConnectionStmt, FlowStmt as V2FlowStmt, GenerateStmt as V2GenerateStmt, ConditionalStmt as V2ConditionalStmt, IntentClause, IntentCall, IntentParams};
pub use v2_board::{BoardV2Ext, BoardBody, BoardBodyExt};
pub use interfaces::{InterfaceSignal, InterfaceRequirement, InterfacePerspective, InterfaceInst, SignalDirection};
pub use testbench::{TestbenchDef, SimulationBlock, ScopeDef, StimulusBlock, VerifyBlock, MeasureBlock, 
                    SignalRef, CaptureMode, StimulusAssign, WaveformExpr, Assertion, Measurement, TimeSpec};
pub use safety::{SatisfiesBlock, SatisfiesItem, SatisfiesSpec, SatisfiesVia, SatisfiesDetails, HasSatisfies};
pub use behavioral_models::{BehavioralModel, OptimizationStrategy, ComponentKnowledge};

// Add tests module
#[cfg(test)]
mod tests; 