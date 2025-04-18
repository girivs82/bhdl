// bhdl-ast/src/lib.rs

use std::ops::Range;
use tree_sitter::Point;

// --- Core Types ---

/// Represents a span in the source code (byte range).
/// Useful for diagnostics and linking AST nodes back to source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start_byte: usize,
    pub end_byte: usize,
    // Add start_point, end_point (line/column) from tree-sitter::Node
    pub start_point: Point,
    pub end_point: Point,
}

impl Span {
    /// Creates a new Span covering the range from the start of `start_span` to the end of `end_span`.
    /// Useful for combining spans of child nodes to get the span of a parent.
    pub fn union(start_span: Span, end_span: Span) -> Self {
        Span {
            start_byte: start_span.start_byte,
            end_byte: end_span.end_byte,
            start_point: start_span.start_point,
            end_point: end_span.end_point,
        }
    }

    pub fn range(&self) -> Range<usize> {
        self.start_byte..self.end_byte
    }
}

/// Represents an identifier in the source code.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier {
    pub span: Span,
    pub value: String,
}

/// Represents a physical literal (value + unit).
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicalLiteral {
    pub span: Span,
    pub value_text: String, // Keep original text for precision
    pub unit: String, // The unit identifier (e.g., "kOhm", "Vdc", "MHz")
    // Optional: Parsed numeric value (e.g., f64) if needed later
    // pub value: Option<f64>,
}

/// Represents an integer literal in the source code.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntegerLiteral {
    pub span: Span,
    pub value: u64, // Assuming u64 for now
}

/// Enum to represent the property in a member access (Identifier or Integer)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemberAccessProperty {
    Identifier(Identifier),
    Integer(IntegerLiteral),
}

// Add structs for the new literals we didn't define initially

#[derive(Debug, Clone, PartialEq)]
pub struct FloatLiteral {
    pub span: Span,
    pub value_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BooleanLiteral {
    pub span: Span,
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StringLiteral {
    pub span: Span,
    pub value: String, // Decoded string value (escapes handled)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CharLiteral {
    pub span: Span,
    pub value: String, // Decoded char value (escapes handled)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumValueLiteral {
    pub span: Span,
    pub type_name: Identifier,
    pub value_name: Identifier,
}


// --- Top Level ---

/// Represents the root of a BHDL file.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub span: Span, // Span covering the whole file
    pub items: Vec<TopLevelItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TopLevelItem {
    BoardDefinition(BoardDefinition),
    ComponentDefinition(ComponentDefinition),
    ModuleDefinition(ModuleDefinition),
    TypedefDefinition(TypedefDefinition),
    PropertySetDefinition(PropertySetDefinition),
    InterfaceDefinition(InterfaceDefinition),
    NetClassDefinition(NetClassDefinition),
    ViaStyleDefinition(ViaStyleDefinition),
    GenerateBlock(GenerateBlock),
    AssignmentStatement(AssignmentStatement), // e.g., local x = 5;
    ImportStatement(ImportStatement),
    Comment(CommentNode), // Decide if comments are part of AST
    // Maybe ExpressionStatement for things like function calls?
}

// Placeholder for comments if needed
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommentNode {
    pub span: Span,
    pub content: String,
}


// --- Import Statement ---

#[derive(Debug, Clone, PartialEq)]
pub struct ImportStatement {
    pub span: Span,
    pub path: ImportPath,
    pub items: Option<ImportItems>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportPath {
    pub span: Span,
    pub segments: Vec<Identifier>, // e.g., ["StandardLibrary", "Components"]
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportItems {
    All(Span), // Represents the '*'
    List(ImportList),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportList {
    pub span: Span, // Span including braces {}
    pub items: Vec<Identifier>,
}


// --- Assignment Statement ---

#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentStatement {
    pub span: Span,
    pub left: Identifier, // Grammar currently only supports simple identifier
    pub eq_span: Span,
    pub right: Expression,
}

// --- Property Assignment (Used everywhere now with '=') ---

/// Represents `name = value` assignment used in various blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyAssignmentEquals {
    pub span: Span,
    pub name: Identifier,
    pub eq_span: Span, // Span of the '=' sign
    pub value: Expression, // Value can be any expression
}

// --- Pin Property Assignment (Used in pin declarations) ---

/// Represents `name = value` assignment used inline in pin declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct PinPropertyAssignment {
    pub span: Span,
    pub name: Identifier,
    pub eq_span: Span, // Span of the '=' sign
    pub value: Expression,
}


// --- Pin Mapping (Used in Interface Instantiation) ---

/// Represents `LogicalPin = PhysicalPin` within a pin_map block.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PinMapping {
    pub span: Span,
    pub logical_pin: Identifier,
    pub eq_span: Span,
    pub physical_pin: Identifier,
}

/// Represents the `pin_map = { ... }` property in an interface instantiation.
#[derive(Debug, Clone, PartialEq)]
pub struct PinMapProperty {
    pub span: Span, // Span including pin_map = { ... }
    pub pin_map_kw_span: Span,
    pub eq_span: Span,
    pub mappings: Vec<PinMapping>,
    pub body_span: Span, // Span including braces {}
}

// --- Net Declarations ---

/// Represents a `net Name[bus]?: Type?;` declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct NetDeclaration {
    pub span: Span,
    pub net_kw_span: Span,
    pub name: Identifier,
    pub bus_specifier: Option<BusSpecifier>,
    pub net_type: Option<TypeSpecifier>, // Changed from TypeName
    // Note: Semicolon span might be needed if grammar requires it consistently
}

/// Represents a `nets { ... }` block.
#[derive(Debug, Clone, PartialEq)]
pub struct NetsBlock {
    pub span: Span, // Span including nets { ... }
    pub nets_kw_span: Span,
    pub declarations: Vec<NetDeclaration>,
    pub body_span: Span, // Span including braces {}
}

// --- Constraint Targets ---

/// Represents a target for a `constrain` statement (Identifier, Member, Subscript).
#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintTarget {
    Identifier(Identifier),
    MemberAccess(Box<MemberAccessExpression>),
    SubscriptAccess(Box<SubscriptAccessExpression>),
}

// --- Type Specifiers ---

/// Represents a type usage (Base, Scoped, Power specific). Matches grammar `_type_specifier`.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeSpecifier {
    Base(BaseType), // signal, power, ground keywords
    Scoped(ScopedTypeName), // Identifier or path like Types.cmos_3v3
    Power(PowerTypeSpecifier), // power(voltage, current) - Placeholder
    // TODO: Add other complex type specifiers if needed (e.g., signal(subtype))
}

/// Placeholder for `power(...)` syntax if needed.
/// Grammar currently doesn't explicitly parse parameters here.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerTypeSpecifier {
    pub span: Span,
    pub power_kw_span: Span,
    // pub voltage: Option<Expression>,
    // pub current: Option<Expression>,
}

/// Represents a base type keyword: signal, power, ground.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BaseType {
    pub span: Span,
    pub kind: BaseTypeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseTypeKind { Signal, Power, Ground }

/// Represents a potentially scoped type name like `MyType` or `Lib.Types.MyType`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopedTypeName {
    pub span: Span,
    pub path: Vec<Identifier>, // Segments of the type path
}


// --- Definitions ---

#[derive(Debug, Clone, PartialEq)]
pub struct BoardDefinition {
    pub span: Span,
    pub name: Identifier,
    pub parameters_decl: Option<DeclarationParameterList>,
    pub body: Vec<BoardItem>,
    pub body_span: Span, // Span including braces {}
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDefinition {
    pub span: Span,
    pub name: Identifier,
    pub parameters_decl: Option<DeclarationParameterList>,
    pub body: Vec<ModuleItem>,
    pub body_span: Span, // Span including braces {}
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentDefinition {
    pub span: Span,
    pub name: Identifier,
    pub parameters_decl: Option<DeclarationParameterList>,
    pub body: Vec<ComponentItem>, // Body is now mandatory {} brace pair
    pub body_span: Span, // Span including braces {}
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypedefDefinition {
    pub span: Span,
    pub name: Identifier,
    pub extends: Option<ScopedTypeName>, // Optional parent type (updated type)
    pub properties: Vec<PropertyAssignmentEquals>, // Updated
    pub body_span: Span, // Span including braces {}
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertySetDefinition {
    pub span: Span,
    pub name: Identifier,
    pub properties: Vec<PropertyAssignmentEquals>, // Updated
    pub body_span: Span, // Span including braces {}
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDefinition {
    pub span: Span,
    pub name: Identifier,
    pub parameters_decl: Option<DeclarationParameterList>,
    pub body: Vec<InterfaceItem>,
    pub body_span: Span, // Span including braces {}
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetClassDefinition {
    pub span: Span,
    pub name: Identifier,
    pub properties: Vec<PropertyAssignmentEquals>, // Updated
    pub body_span: Span, // Span including braces {}
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViaStyleDefinition {
    pub span: Span,
    pub name: Identifier,
    pub properties: Vec<PropertyAssignmentEquals>, // Updated
    pub body_span: Span, // Span including braces {}
}

// --- Definition Items (Enums for items allowed within definitions) ---

#[derive(Debug, Clone, PartialEq)]
pub enum BoardItem {
    ParametersBlock(ParametersBlock),
    PortsBlock(PortsBlock),
    ComponentsBlock(ComponentsBlock),
    NetsBlock(NetsBlock), // Added NetsBlock
    ConnectionsBlock(ConnectionsBlock),
    LayerStackupBlock(LayerStackupBlock),
    DefaultDesignRulesBlock(DefaultDesignRulesBlock),
    ConstraintStatement(ConstraintStatement),
    GenerateBlock(GenerateBlock),
    ComponentDefinition(ComponentDefinition), // Allow nested definitions?
    ModuleDefinition(ModuleDefinition),       // Allow nested definitions?
    TypedefDefinition(TypedefDefinition),
    InterfaceDefinition(InterfaceDefinition),
    NetClassDefinition(NetClassDefinition),
    ViaStyleDefinition(ViaStyleDefinition),
    PropertySetDefinition(PropertySetDefinition),
    Comment(CommentNode),
    // AssignmentStatement?
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleItem {
    ParametersBlock(ParametersBlock),
    PortsBlock(PortsBlock),
    ComponentsBlock(ComponentsBlock),
    NetsBlock(NetsBlock), // Added NetsBlock
    ConnectionsBlock(ConnectionsBlock),
    GenerateBlock(GenerateBlock),
    ConstraintStatement(ConstraintStatement),
    ComponentDefinition(ComponentDefinition), // Nested defs
    ModuleDefinition(ModuleDefinition),       // Nested defs
    TypedefDefinition(TypedefDefinition),
    InterfaceDefinition(InterfaceDefinition),
    PropertySetDefinition(PropertySetDefinition),
    Comment(CommentNode),
    // AssignmentStatement?
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentItem {
    ParametersBlock(ParametersBlock),
    PinsBlock(PinsBlock),
    InterfacesBlock(InterfacesBlock),
    GenerateBlock(GenerateBlock),
    ConstraintStatement(ConstraintStatement),
    Comment(CommentNode),
    // AssignmentStatement?
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterfaceItem {
    ParametersBlock(ParametersBlock),
    PinsBlock(PinsBlock), // Note: Spec uses 'pins' inside interface
    GenerateBlock(GenerateBlock),
    Comment(CommentNode),
    // AssignmentStatement?
}


// --- Blocks within Definitions ---

#[derive(Debug, Clone, PartialEq)]
pub struct ParametersBlock {
    pub span: Span, // Span including braces {}
    pub parameters: Vec<ParameterDeclaration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeclarationParameterList { // The () list in definitions
    pub span: Span, // Span including parens ()
    pub parameters: Vec<ParameterDeclaration>, // Uses the same decl as in block
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortsBlock {
    pub span: Span, // Span including braces {}
    pub ports: Vec<PinPortDeclaration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PinsBlock { // Used in Component, Interface
    pub span: Span, // Span including braces {}
    pub pins: Vec<PinPortDeclaration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentsBlock {
    pub span: Span, // Span including braces {}
    pub instantiations: Vec<ComponentInstantiation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionsBlock {
    pub span: Span, // Span including braces {}
    pub connections: Vec<ConnectionStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfacesBlock { // Used in Component
    pub span: Span, // Span including braces {}
    pub interfaces: Vec<InterfaceInstantiation>, // Uses the updated definition
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerStackupBlock {
    pub span: Span, // Span including braces {}
    pub layers: Vec<LayerDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerDefinition {
    pub span: Span,
    pub layer_kw_span: Span,
    pub name: Identifier,
    pub colon_span: Span,
    pub properties: Vec<PropertyAssignmentEquals>, // Updated
    pub body_span: Span, // Span including braces {}
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefaultDesignRulesBlock {
    pub span: Span, // Span including braces {}
    pub rules: Vec<PropertyAssignmentEquals>, // Updated
    pub body_span: Span, // Span including braces {}
    // TODO: Consider adding assign_net_class if grammar supports it here
}


// --- Generate Block ---

#[derive(Debug, Clone, PartialEq)]
pub struct GenerateBlock {
    pub span: Span, // Span including braces {}
    pub items: Vec<GenerateItem>, // What can be generated? Depends on context
}

#[derive(Debug, Clone, PartialEq)]
pub enum GenerateItem {
    ForLoop(GenerateForLoop),
    // IfStatement(GenerateIf), // If grammar supports generate if
    // Nested items allowed by the context (e.g., PinPortDeclaration in PinsBlock)
    // Need a way to represent these... maybe just allow specific block items?
    PinPortDeclaration(PinPortDeclaration),
    ComponentInstantiation(ComponentInstantiation),
    ConnectionStatement(ConnectionStatement),
    ConstraintStatement(ConstraintStatement),
    // ... add others as needed based on where generate is allowed ...
    Comment(CommentNode),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenerateForLoop {
    pub span: Span,
    pub variable: Identifier,
    pub in_span: Span,
    pub iterator: Expression, // e.g., RangeExpression or Identifier
    pub body: GenerateBlock, // For loop body always uses GenerateBlock
}

// --- Constraints ---

#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintStatement {
    pub span: Span,
    pub constrain_kw_span: Span,
    pub targets: Vec<ConstraintTarget>, // Changed from single target
    pub targets_span: Span, // Span for (...) including parens
    pub body: Vec<PropertyAssignmentEquals>, // Changed from PropertyAssignment
    pub body_span: Span, // Span including braces {}
}


// --- Interface Instantiation (used inside InterfacesBlock) ---
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceInstantiation {
    pub span: Span,
    pub instance_name: Identifier, // Renamed from name
    pub colon_span: Span, // Kept
    pub interface_type: ScopedTypeName, // Renamed from iface, changed type
    pub properties: Vec<PropertyAssignmentEquals>, // Renamed from parameters, changed type
    pub pin_map_property: Option<PinMapProperty>, // Renamed from pin_map, changed type
    pub body_span: Span, // Span covering the { ... } block, ADDED
}


// --- Expressions (Add missing types) ---

/// Represents various kinds of expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Identifier(Identifier),
    PhysicalLiteral(PhysicalLiteral),
    IntegerLiteral(IntegerLiteral),
    FloatLiteral(FloatLiteral),
    BooleanLiteral(BooleanLiteral),
    StringLiteral(StringLiteral),
    CharLiteral(CharLiteral),
    EnumValueLiteral(EnumValueLiteral), // MyType'Value
    Binary(Box<BinaryExpression>),
    Unary(Box<UnaryExpression>),
    Ternary(Box<TernaryExpression>), // Added
    Parenthesized(Box<ParenthesizedExpression>),
    FunctionCall(Box<FunctionCallExpression>),
    MemberAccess(Box<MemberAccessExpression>),
    SubscriptAccess(Box<SubscriptAccessExpression>), // Ident[Idx] or Ident[H:L]
    Range(Box<RangeExpression>), // Lower op Upper
    // TODO: Array literal? [a, b, c]
    // TODO: Map/Struct literal? { name: val, ... }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TernaryExpression {
    pub span: Span,
    pub condition: Expression,
    pub question_span: Span,
    pub true_value: Expression,
    pub colon_span: Span,
    pub false_value: Expression,
}


// Expression Structs (using Box for recursive types)

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryExpression {
    pub span: Span,
    pub op_span: Span,
    pub op: BinaryOperator,
    pub left: Expression,
    pub right: Expression,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinaryOperator {
    Add, Sub, Mul, Div, // Arithmetic
    Eq, Neq, Lt, Lte, Gt, Gte, // Comparison
    LogicalAnd, LogicalOr, // Logical
    // TODO: Bitwise operators (&, |, ^) if added to grammar
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnaryExpression {
    pub span: Span,
    pub op_span: Span,
    pub op: UnaryOperator,
    pub operand: Expression,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOperator {
    Not, Negate,
    // TODO: Bitwise not (~) if added to grammar
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParenthesizedExpression {
    pub span: Span,
    pub expression: Expression,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCallExpression {
    pub span: Span,
    pub function: Expression, // e.g., Identifier or MemberAccess
    pub arguments: Vec<Argument>,
    pub arg_list_span: Span, // Span covering the (...) including parens
}

/// Argument in a function call. Can be positional or named.
#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    pub span: Span,
    pub name: Option<Identifier>, // For named arguments (name = value)
    pub value: Expression,
}

/// Represents a member access expression (e.g., `a.b` or `Comp1.pin`).
#[derive(Debug, Clone, PartialEq)]
pub struct MemberAccessExpression {
    pub span: Span,
    pub object: Expression,
    pub dot_span: Span,
    pub property: MemberAccessProperty,
}

/// Represents accessing an element or slice of a bus/array.
#[derive(Debug, Clone, PartialEq)]
pub struct SubscriptAccessExpression {
    pub span: Span,
    pub object: Expression,
    pub index: Box<BusSpecifier>, // Use BusSpecifier for index/slice
    pub index_span: Span, // Span covering [...] including brackets
}

/// Represents a range expression (e.g., `0 to 7`, `min .. max`).
#[derive(Debug, Clone, PartialEq)]
pub struct RangeExpression {
    pub span: Span,
    pub op_span: Span,
    pub op: RangeOperator,
    pub lower: Expression,
    pub upper: Expression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RangeOperator {
    // Exclusive, // .. - Is this used in BHDL spec? Grammar has to, upto
    InclusiveTo, // to
    InclusiveUpTo, // upto
}


// --- Declarations (Already mostly existed, ensure complete) ---

/// Specifies a bus index or slice, used in pin/port declarations,
/// component instantiations, and subscript expressions.
#[derive(Debug, Clone, PartialEq)]
pub struct BusSpecifier {
    pub span: Span, // Span covering the [...] including brackets
    pub high: Expression, // Single index if low is None
    pub colon_span: Option<Span>,
    pub low: Option<Expression>, // Slice if Some
}

/// Declaration of a parameter within a parameters block or definition parameter list.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterDeclaration {
    pub span: Span,
    pub name: Identifier,
    pub param_type: Option<TypeSpecifier>, // Optional type spec
    pub colon_span: Option<Span>, // Span for ':' if type is present
    pub eq_span: Span, // '=' is now mandatory for the value
    pub value: Expression, // Assigned value (required)
}

/// Declaration of a pin or port within a pins/ports block.
#[derive(Debug, Clone, PartialEq)]
pub struct PinPortDeclaration {
    pub span: Span,
    pub is_port: bool, // true if 'port', false if 'pin' (or keyword omitted)
    pub name: Identifier,
    pub bus_specifier: Option<BusSpecifier>, // Optional bus specifier `[H:L]` or `[I]`
    pub direction: PinDirection,
    pub type_specifier: TypeSpecifier, // Replaces PinPortKind
    pub properties: Vec<PinPropertyAssignment>, // Changed from optional block to Vec
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinDirection { In, Out, Inout }


// --- Instantiations & Connections (Already mostly existed) ---

/// Instantiation of a component within a components block.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentInstantiation {
    pub span: Span,
    pub component_type: ScopedTypeName, // Type of component being instantiated (updated type)
    pub instance_name: Identifier,
    pub instance_bus: Option<BusSpecifier>, // Optional array instantiation `U[0:3]`
    pub properties: Vec<PropertyAssignmentEquals>, // Replaced parameters Option<ComponentParameters>
    pub properties_span: Span, // Span covering { ... } including braces
}

/// Represents a connection statement in the `connections` block.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatement {
    InterfaceConnection { // Target <=> Target
        span: Span,
        source: ConnectionTarget,
        op_span: Span, // Span for <=>, Operator implicit (<=>)
        target: ConnectionTarget,
    },
    PinsToPins { // Vec<Target> -> Vec<Target>
        span: Span,
        sources: Vec<ConnectionTarget>,
        op_span: Span, // Span for ->, Operator implicit (->)
        targets: Vec<ConnectionTarget>,
    },
    PinsToNet { // Vec<Target> -> Target (Net)
        span: Span,
        sources: Vec<ConnectionTarget>,
        op_span: Span, // Span for ->, Operator implicit (->)
        target_net: ConnectionTarget, // Must resolve to a net
    },
    NetToPins { // Target (Net) -> Vec<Target>
        span: Span,
        source_net: ConnectionTarget, // Must resolve to a net
        op_span: Span, // Span for ->, Operator implicit (->)
        targets: Vec<ConnectionTarget>,
    },
    // Removed old variants like MultiTarget, Series, Parallel
}

/// Represents the target of a connection (Identifier, MemberAccess, SubscriptAccess).
/// Renamed from ConnectionEndpoint.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionTarget {
    Identifier(Identifier),
    MemberAccess(Box<MemberAccessExpression>),
    SubscriptAccess(Box<SubscriptAccessExpression>),
}


// --- Tests (Keep existing, but update parameter declaration test) ---
#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_span() -> Span {
        Span { start_byte: 0, end_byte: 0, start_point: Point::new(0, 0), end_point: Point::new(0, 0) }
    }

    fn dummy_ident(name: &str) -> Identifier {
        Identifier {
            span: dummy_span(),
            value: name.to_string(),
        }
    }

    fn dummy_scoped_type(path: Vec<&str>) -> ScopedTypeName {
        ScopedTypeName {
            span: dummy_span(),
            path: path.into_iter().map(dummy_ident).collect(),
        }
    }

    #[test]
    fn test_expression_structs() {
        // Test basic literals
        let _int_lit = Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 10 });
        let _phys_lit = Expression::PhysicalLiteral(PhysicalLiteral { span: dummy_span(), value_text: "4.7".to_string(), unit: "kOhm".to_string() });
        let _bool_lit = Expression::BooleanLiteral(BooleanLiteral { span: dummy_span(), value: true });
        let _string_lit = Expression::StringLiteral(StringLiteral { span: dummy_span(), value: "hello".to_string() });

        // Test binary expression
        let _binary_expr = Expression::Binary(Box::new(BinaryExpression {
            span: dummy_span(),
            op_span: dummy_span(),
            op: BinaryOperator::Add,
            left: Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 5 }),
            right: Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 3 }),
        }));

        // Test unary expression
        let _unary_expr = Expression::Unary(Box::new(UnaryExpression {
            span: dummy_span(),
            op_span: dummy_span(),
            op: UnaryOperator::Negate,
            operand: Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 10 }),
        }));

        // Test parenthesized expression
        let _paren_expr = Expression::Parenthesized(Box::new(ParenthesizedExpression {
            span: dummy_span(),
            expression: Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 42 }),
        }));

        // Test function call
        let _func_call = Expression::FunctionCall(Box::new(FunctionCallExpression {
            span: dummy_span(),
            function: Expression::Identifier(dummy_ident("my_func")),
            arguments: vec![
                Argument { span: dummy_span(), name: None, value: Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 1 }) },
                Argument { span: dummy_span(), name: Some(dummy_ident("param")), value: Expression::BooleanLiteral(BooleanLiteral { span: dummy_span(), value: false }) }
            ],
            arg_list_span: dummy_span(),
        }));

        // Test member access
        let _member_access = Expression::MemberAccess(Box::new(MemberAccessExpression {
            span: dummy_span(),
            object: Expression::Identifier(dummy_ident("my_obj")),
            dot_span: dummy_span(),
            property: MemberAccessProperty::Identifier(dummy_ident("field")),
        }));

        // Test subscript access (single index)
        let _subscript_single = Expression::SubscriptAccess(Box::new(SubscriptAccessExpression {
            span: dummy_span(),
            object: Expression::Identifier(dummy_ident("my_array")),
            index: Box::new(BusSpecifier {
                 span: dummy_span(),
                 high: Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 3 }),
                 colon_span: None,
                 low: None
            }),
            index_span: dummy_span(),
        }));

         // Test subscript access (slice)
         let _subscript_slice = Expression::SubscriptAccess(Box::new(SubscriptAccessExpression {
            span: dummy_span(),
            object: Expression::Identifier(dummy_ident("my_bus")),
            index: Box::new(BusSpecifier {
                 span: dummy_span(),
                 high: Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 7 }),
                 colon_span: Some(dummy_span()),
                 low: Some(Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 0 }))
            }),
            index_span: dummy_span(),
        }));


        // Test range expression
        let _range_expr = Expression::Range(Box::new(RangeExpression {
            span: dummy_span(),
            op_span: dummy_span(),
            op: RangeOperator::InclusiveTo,
            lower: Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 0 }),
            upper: Expression::Identifier(dummy_ident("MAX_VAL")),
        }));

        // TODO: Test TernaryExpression

        println!("Expression AST structs seem okay.");
    }

    #[test]
    fn test_declaration_structs() {
        // Test ParameterDeclaration
        let _param_decl = ParameterDeclaration {
            span: dummy_span(),
            name: dummy_ident("InputVoltage"),
            // Updated: Use TypeSpecifier/ScopedTypeName
            param_type: Some(TypeSpecifier::Scoped(dummy_scoped_type(vec!["types", "voltage"]))),
            colon_span: Some(dummy_span()),
            value: Expression::PhysicalLiteral(PhysicalLiteral { span: dummy_span(), value_text: "12".to_string(), unit: "Vdc".to_string() }),
            eq_span: dummy_span(),
        };
        assert_eq!(_param_decl.name.value, "InputVoltage");
        // Add assertion for type if needed

        // Test PinPortDeclaration (Input Signal)
        let _pin_decl_in = PinPortDeclaration {
            span: dummy_span(),
            is_port: false,
            name: dummy_ident("CLK"),
            bus_specifier: None,
            direction: PinDirection::In,
            type_specifier: TypeSpecifier::Base(BaseType { span: dummy_span(), kind: BaseTypeKind::Signal }),
            properties: vec![], // Updated property field
        };
         match _pin_decl_in.type_specifier {
            TypeSpecifier::Base(BaseType { kind, .. }) => assert_eq!(kind, BaseTypeKind::Signal),
            _ => panic!("Incorrect pin type"),
        }

        // Test PinPortDeclaration (Ground)
        let _pin_decl_gnd = PinPortDeclaration {
            span: dummy_span(),
            is_port: false,
            name: dummy_ident("GND"),
            bus_specifier: None,
            direction: PinDirection::In, // Direction is still required by builder, even if ground
            type_specifier: TypeSpecifier::Base(BaseType { span: dummy_span(), kind: BaseTypeKind::Ground }),
            properties: vec![],
        };
         match _pin_decl_gnd.type_specifier {
            TypeSpecifier::Base(BaseType { kind, .. }) => assert_eq!(kind, BaseTypeKind::Ground),
            _ => panic!("Incorrect pin type"),
        }

         // Test PinPortDeclaration (Output Power with Bus and Properties)
         let _port_decl_power = PinPortDeclaration {
            span: dummy_span(),
            is_port: true, // It's a port
            name: dummy_ident("VOUT"),
            bus_specifier: Some(BusSpecifier {
                span: dummy_span(),
                high: Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 1 }),
                colon_span: Some(dummy_span()),
                low: Some(Expression::IntegerLiteral(IntegerLiteral { span: dummy_span(), value: 0 })),
            }),
            direction: PinDirection::Out,
            type_specifier: TypeSpecifier::Base(BaseType { span: dummy_span(), kind: BaseTypeKind::Power }),
            properties: vec![ // Added properties
                PinPropertyAssignment {
                    span: dummy_span(),
                    name: dummy_ident("voltage"),
                    eq_span: dummy_span(),
                    value: Expression::PhysicalLiteral(PhysicalLiteral { span: dummy_span(), value_text: "3.3".to_string(), unit: "V".to_string() })
                }
            ],
        };
         match _port_decl_power.type_specifier {
            TypeSpecifier::Base(BaseType { kind, .. }) => assert_eq!(kind, BaseTypeKind::Power),
            _ => panic!("Incorrect pin type"),
        }
        assert_eq!(_port_decl_power.properties.len(), 1);


        // Test ComponentInstantiation
        let _comp_inst = ComponentInstantiation {
            span: dummy_span(),
            component_type: dummy_scoped_type(vec!["Resistor"]),
            instance_name: dummy_ident("R1"),
            instance_bus: None,
            properties: vec![ // Updated property field
                PropertyAssignmentEquals {
                    span: dummy_span(),
                    name: dummy_ident("value"),
                    eq_span: dummy_span(),
                    value: Expression::PhysicalLiteral(PhysicalLiteral { span: dummy_span(), value_text: "10".to_string(), unit: "kOhm".to_string() }),
                }
            ],
            properties_span: dummy_span(), // Added span
        };

        // Test ConnectionStatement (InterfaceConnection)
        let _conn_stmt_iface = ConnectionStatement::InterfaceConnection {
            span: dummy_span(),
            source: ConnectionTarget::MemberAccess(Box::new(MemberAccessExpression { // Use MemberAccess for U1.I2C
                 span: dummy_span(),
                 object: Expression::Identifier(dummy_ident("U1")),
                 dot_span: dummy_span(),
                 property: MemberAccessProperty::Identifier(dummy_ident("I2C")),
            })),
            op_span: dummy_span(), // Span for <=>
            target: ConnectionTarget::MemberAccess(Box::new(MemberAccessExpression { // Use MemberAccess for U2.I2C
                 span: dummy_span(),
                 object: Expression::Identifier(dummy_ident("U2")),
                 dot_span: dummy_span(),
                 property: MemberAccessProperty::Identifier(dummy_ident("I2C")),
            })),
        };

        // Example for PinsToPins (->)
        let _pins_to_pins = ConnectionStatement::PinsToPins {
            span: dummy_span(),
            sources: vec![ConnectionTarget::MemberAccess(Box::new(MemberAccessExpression {
                span: dummy_span(),
                object: Expression::Identifier(dummy_ident("U1")),
                dot_span: dummy_span(),
                property: MemberAccessProperty::Identifier(dummy_ident("TXD")),
            }))],
            op_span: dummy_span(), // Span for ->
            targets: vec![ConnectionTarget::MemberAccess(Box::new(MemberAccessExpression {
                 span: dummy_span(),
                 object: Expression::Identifier(dummy_ident("U2")),
                 dot_span: dummy_span(),
                 property: MemberAccessProperty::Identifier(dummy_ident("RXD")),
            }))],
        };

        // Example for NetToPins (->)
        let _net_to_pins = ConnectionStatement::NetToPins {
            span: dummy_span(),
            source_net: ConnectionTarget::Identifier(dummy_ident("DATA_BUS")), // Example: Net name
            op_span: dummy_span(), // Span for ->
            targets: vec![
                ConnectionTarget::MemberAccess(Box::new(MemberAccessExpression {
                    span: dummy_span(),
                    object: Expression::Identifier(dummy_ident("MEM")),
                    dot_span: dummy_span(),
                    property: MemberAccessProperty::Identifier(dummy_ident("D")),
                })),
                ConnectionTarget::MemberAccess(Box::new(MemberAccessExpression {
                    span: dummy_span(),
                    object: Expression::Identifier(dummy_ident("CPU")),
                    dot_span: dummy_span(),
                    property: MemberAccessProperty::Identifier(dummy_ident("D")),
                })),
            ],
        };


        println!("Declaration AST structs seem okay.");
    }
}
