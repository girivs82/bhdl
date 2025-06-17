use rowan::Language;
use logos::Logos;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Logos,
)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // Tokens
    WHITESPACE = 0, // Trivia
    COMMENT,        // Trivia
    IDENT,          // Identifier
    NUMBER,         // Numeric literal (incl. units)
    STRING,         // String literal
    ERROR_TOKEN,    // Unrecognized token

    // Punctuation
    L_BRACE, // {
    R_BRACE, // }
    L_PAREN, // (
    R_PAREN, // )
    L_BRACKET, // [
    R_BRACKET, // ]
    DOT,     // .
    COMMA,   // ,
    COLON,   // :
    SEMI,    // ;
    ARROW,   // ->
    BI_ARROW, // <->
    FLOW_OP, // |>
    INTERFACE_OP, // <=>
    EQ,      // =
    PLUS,    // +
    MINUS,   // -
    STAR,    // *
    SLASH,   // /
    PERCENT, // %
    AMPERSAND, // &
    PIPE,    // |
    CARET,   // ^
    BANG,    // !
    TILDE,   // ~
    L_ANGLE, // <
    R_ANGLE, // >
    AT,      // @
    QUESTION, // ?
    EQEQ,    // ==
    NEQ,     // !=
    LTEQ,    // <=
    GTEQ,    // >=
    AMPAMP,  // &&
    PIPEPIPE,// ||
    LSHIFT,  // <<
    RSHIFT,  // >>
    IF_CONNECT, // <=> (legacy - use INTERFACE_OP)

    // Keywords
    IMPORT_KW, // import
    BOARD_KW,  // board
    MODULE_KW, // module
    TYPEDEF_KW,   // typedef
    STRUCT_KW,    // struct
    ENUM_KW,      // enum
    INTERFACE_KW, // interface
    COMPONENT_KW, // component (for direct instantiation in v2.0)
    NET_KW,        // net
    LAYER_STACKUP_KW, // layer_stackup
    LAYER_KW,         // layer
    DEFAULT_DESIGN_RULES_KW, // default_design_rules
    CONSTRAIN_KW,  // constrain
    GENERATE_KW,   // generate
    FOR_KW,        // for
    IN_KW,         // in
    OUT_KW,        // out
    INOUT_KW,      // inout
    INPUT_KW,      // input
    OUTPUT_KW,     // output
    SIGNAL_KW,     // signal (base type)
    WIRE_KW,       // wire
    TRI_KW,        // tri
    TRIREG_KW,     // trireg
    UWIRE_KW,      // uwire
    POWER_KW,      // power (base type)
    GROUND_KW,     // ground (base type)
    CLOCK_KW,      // clock
    FUNCTIONS_KW,   // functions
    TRUE_KW,
    FALSE_KW,
    PORT_KW,      // port
    CONST_KW,     // const
    ASSIGN_KW,    // assign
    FROM_KW,      // from
    AS_KW,        // as
    TO_KW,        // to
    EXTENDS_KW,   // extends
    IF_KW,        // if
    ELSE_KW,      // else
    WHEN_KW,      // when

    // Keywords for individual items  
    PIN_KW,       // pin
    PARAMETER_KW, // parameter
    CONNECT_KW,   // connect
    ATTRIBUTE_KW, // attribute

    // Nodes (Grammar rules)
    SOURCE_FILE,    // Root node
    IMPORT_STMT,    // import ...;
    IMPORT_PATH,    // Ident.Ident...
    IMPORT_TARGET,  // Final IDENT or { ... } group
    IMPORT_TARGET_GROUP, // { Item, Item ... }
    ALIAS,          // `as AliasName` part of import
    BOARD_DEF,      // board BoardName { ... }
    MODULE_DEF,     // module ModuleName { ... }
    COMPONENT_DEF,  // component ComponentName { ... }
    TYPEDEF_DEF,    // typedef TypeName [extends Base] { ... }
    TYPEDEF_BASE,   // Node wrapping the base type name after extends
    STRUCT_DEF,     // struct definition
    ENUM_DEF,       // enum definition
    INTERFACE_DEF,  // interface InterfaceName { ... }

    // v2.0 Blocks (minimal)
    LAYER_STACKUP_BLOCK,
    LAYER_DEF,          // layer NAME { ... }
    DEFAULT_DESIGN_RULES_BLOCK,
    CONSTRAIN_BLOCK,
    GENERATE_BLOCK,     // generate { ... }
    FOR_LOOP_GENERATE, // ADDED
    GENERATE_FOR_BLOCK, // ADDED
    IF_GENERATE,        // ADDED
    GENERATE_STMT,      // generate for ... { ... }
    CONDITIONAL_STMT,   // if (...) { ... } else { ... }
    FLOW_STMT,          // name: flow_expr;
    FLOW_EXPR,          // flow expression with |> operators
    
    // Individual statements for linear regulator syntax
    POWER_DECL,         // power VIN = 12V @ 2A;
    GROUND_DECL,        // ground GND;
    CONNECTION_TARGET,  // U1.input or vin_rail

    // Items within Blocks
    PARAM_DECL,        // const param_name: type = value;
    PORT_DECL,         // port_name: direction type ...;
    COMPONENT_INST,    // Resistor R1 { ... }
    PARAM_ASSIGN_BLOCK,// The `(...)` or `{...}` part in component instantiation
    PARAM_ASSIGN,      // param_name = value (inside PARAM_ASSIGN_BLOCK)
    COMPONENT_TYPE,    // Type name used in component instantiation
    NET_DECL,          // net net_name[range]: type;
    NET_TYPE,          // The type keyword used in net decl (SIGNAL_KW etc)
    CONNECTION_STMT,   // PinRef -> PinRef; OR NetRef -> PinRef; OR PinRef -> NetRef; OR AssignStmt
    ASSIGN_STMT,       // assign NetRef = Expression;
    NET_REF,           // Reference to a net name
    PIN_REF,           // instance.pin[range] or net_name[range]
    CONSTRAINT_TARGET, // Node for the target(s) in constrain()
    BUS_SUFFIX,        // [index] or [high:low]
    RANGE_EXPR,        // start to end
    VALUE,             // Generic value node (number, string, bool, etc.)
    TYPE_REF,          // Reference to a type (e.g., cmos_3v3 or signal(cmos_3v3))
    TYPE_SPECIFIER,    // (specifier_name)
    PIN_PROPERTIES,    // Pin properties
    PORT_DIRECTION,    // input, output, inout
    EXPRESSION,        // Generic expression node (might wrap others)
    BINARY_EXPR,       // Node for binary operations (lhs op rhs)
    PREFIX_EXPR,       // Node for prefix unary operations (op rhs)
    IDENT_REF,         // Reference to an identifier
    PATH_REF,          // Reference like scope::identifier
    TYPE_PARAMS,       // Added for type parameters like signal(param)
    PIN_BUS_SUFFIX,    // Node for the [high:low] or [index] suffix
    TERNARY_EXPR,      // condition ? true_expr : false_expr
    FUNCTION_CALL_EXPR, // name(arg1, arg2)
    ARGUMENT_LIST,     // (arg1, arg2)
    UNIT_IDENTIFIER,   // A unit like kOhm, Vdc, pct, etc.
    
    // Unit token kinds (for v2.0 unit support)
    V_UNIT, MV_UNIT, UV_UNIT,
    A_UNIT, MA_UNIT, UA_UNIT,
    OHM_UNIT, KOHM_UNIT, MOHM_UNIT,
    F_UNIT, UF_UNIT, NF_UNIT, PF_UNIT,
    HZ_UNIT, KHZ_UNIT, MHZ_UNIT, GHZ_UNIT,
    W_UNIT, MW_UNIT, UW_UNIT,
    S_UNIT, MS_UNIT, US_UNIT, NS_UNIT,
    PARAM_LIST,        // Parameter list (param1=value1, param2=value2)
    ARRAY_EXPR,        // Array expression [item1, item2, ...]

    // References
    SIMPLE_IDENT_REF, // Reference consisting of a single identifier (could be Net, Pin, Port, etc.)

    // Added for Connection Statements
    CONNECTION_LHS,    // Wrapper node for LHS refs
    CONNECTION_RHS,    // Wrapper node for RHS refs

    // ERROR must be the last variant for the assertion in kind_from_raw
    ERROR = 65534, // Represents a parsing error node
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BhdlLanguage {}

impl Language for BhdlLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 < SyntaxKind::ERROR as u16);
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<BhdlLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<BhdlLanguage>;
// pub type SyntaxElement = rowan::SyntaxElement<BhdlLanguage>; // Commented out - unused
// pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<BhdlLanguage>; // Commented out - unused
// pub type SyntaxElementChildren = rowan::SyntaxElementChildren<BhdlLanguage>; // Commented out - unused

// Helper trait for typed AST nodes (example)
// pub trait AstNode {
//     fn can_cast(kind: SyntaxKind) -> bool where Self: Sized;
//     fn cast(node: SyntaxNode) -> Option<Self> where Self: Sized;
//     fn syntax(&self) -> &SyntaxNode;
// }

// Add more imports if needed for extensions
// use rowan::NodeOrToken;

// Example extension trait for SyntaxNode (optional but useful)
// pub trait SyntaxNodeExt {
//     fn child<N: AstNode>(&self) -> Option<N>;
//     fn children<N: AstNode>(&self) -> impl Iterator<Item = N>;
//     fn field_token(&self, kind: SyntaxKind) -> Option<SyntaxToken>;
// }

// impl SyntaxNodeExt for SyntaxNode {
//     fn child<N: AstNode>(&self) -> Option<N> {
//         self.children().find_map(N::cast)
//     }

//     fn children<N: AstNode>(&self) -> impl Iterator<Item = N> {
//         self.children().filter_map(N::cast)
//     }

//     fn field_token(&self, kind: SyntaxKind) -> Option<SyntaxToken> {
//         self.children_with_tokens()
//             .filter_map(|element| element.into_token())
//             .find(|token| token.kind() == kind)
//     }
// } 