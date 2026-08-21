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
    RAW_STRING,     // Raw string literal (r"...", r#"..."#, …) — used to
                    // embed foreign-language source (Rhai design block
                    // bodies) verbatim without escaping.
    ERROR_TOKEN,    // Unrecognized token

    // Punctuation
    L_BRACE, // {
    R_BRACE, // }
    L_PAREN, // (
    R_PAREN, // )
    L_BRACKET, // [
    R_BRACKET, // ]
    DOT,     // .
    DOT_DOT, // ..
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
    PLUS_EQ,  // +=
    MINUS_EQ, // -=
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
    ENTITY_KW, // entity
    TYPEDEF_KW,   // typedef
    STRUCT_KW,    // struct
    ENUM_KW,      // enum
    MATCH_KW,     // match
    INTERFACE_KW, // interface
    COMPONENT_KW, // component (for direct instantiation in v2.0)
    NET_KW,        // net
    LAYER_STACKUP_KW, // layer_stackup
    LAYER_KW,         // layer
    DEFAULT_DESIGN_RULES_KW, // default_design_rules
    DESIGN_KW,     // design (for entity-level operating-point design blocks)
    VARIANT_KW,    // variant (board-level product SKU variants)
    DNP_KW,        // dnp (do-not-populate; inside variant blocks)
    SOCKET_KW,     // socket (composition-pairing inside expansion blocks)
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
    WIRES_KW,      // wires (v0.7: interface wire-mapping block)
    ALIASES_KW,    // aliases (v0.9: function-alias block on entity)
    TRI_KW,        // tri
    TRIREG_KW,     // trireg
    UWIRE_KW,      // uwire
    POWER_KW,      // power (base type)
    GROUND_KW,     // ground (base type)
    SWITCH_KW,     // switch (pin type for switching nodes)
    FEEDBACK_KW,   // feedback (pin type for feedback signals)
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
    ALIAS_KW,     // alias
    TYPE_KW,      // type
    NULL_KW,      // null
    REQUIRE_KW,   // require (for interface requirements)
    OPTIONAL_KW,  // optional (for optional interface signals)
    VIRTUAL_KW,   // virtual (for virtual pins that expand during synthesis)
    PERSPECTIVE_KW, // perspective (for interface perspectives)
    WHERE_KW,     // where (for connection constraints)
    WITH_KW,      // with (for grouped constraints)
    TRAIT_KW,     // trait (for interface traits)
    IMPL_KW,      // impl (for trait implementations on components)
    SAFETY_GOAL_KW,   // safety_goal
    SAFETY_ASSUMPTION_KW, // safety_assumption
    SAFETY_KW,        // safety (top-level `safety X as ns { }` block; entity-level `safety { }` data block)
    FAULT_INJECT_KW,  // fault_inject
    SATISFIES_KW, // satisfies (for safety requirement compliance)
    VIA_KW,       // via (for satisfies declarations)
    EXPANSION_KW, // expansion (for entity expansion blocks)
    INTERNAL_KW,  // internal (for expansion-local nets)
    PLACEMENT_KW, // placement (for entity placement blocks)
    SYMBOL_KW,    // symbol (for schematic symbol definitions)
    LAYOUT_KW,    // layout (for PCB layout definitions)
    BODY_KW,      // body (for symbol body hint)
    LEFT_KW,      // left (symbol pin side)
    RIGHT_KW,     // right (symbol pin side)
    TOP_KW,       // top (symbol pin side)
    BOTTOM_KW,    // bottom (symbol pin side)
    GROUP_KW,     // group (symbol pin group)
    PART_KW,      // part (multi-part symbol)
    PACKAGE_KW,   // package (layout package)
    PART_FAMILY_KW, // part_family (v0.2 catalog declaration)
    // Note: REQUIRE_KW already exists above (line ~116) for interface
    // requirements and design-block constraints; the same keyword is
    // reused for part_family constraint clauses.

    // Testbench keywords
    TESTBENCH_KW,    // testbench
    SIMULATION_KW,   // simulation
    SCOPE_KW,        // scope
    STIMULUS_KW,     // stimulus
    VERIFY_KW,       // verify
    ASSERT_KW,       // assert
    MEASURE_KW,      // measure
    AFTER_KW,        // after
    ALWAYS_KW,       // always
    CAPTURE_KW,      // capture
    TRIGGER_KW,      // trigger
    CONTINUOUS_KW,   // continuous
    ON_CHANGE_KW,    // on_change
    PERIODIC_KW,     // periodic
    SIGNALS_KW,      // signals
    DURATION_KW,     // duration
    TIMESTEP_KW,     // timestep
    SOLVER_KW,       // solver
    TEMPERATURE_KW,  // temperature

    // Keywords for individual items  
    PIN_KW,       // pin
    PARAMETER_KW, // parameter
    CONNECT_KW,   // connect
    ATTRIBUTE_KW, // attribute
    
    // Simulation and optimization keywords
    BEHAVIORAL_MODEL_KW,      // behavioral_model
    OPTIMIZATION_STRATEGY_KW, // optimization_strategy
    COMPONENT_KNOWLEDGE_KW,   // component_knowledge
    SIMULATION_REQUIREMENTS_KW, // simulation_requirements
    TEST_SEQUENCES_KW,        // test_sequences
    MODEL_SELECTOR_KW,        // model_selector

    // Scalability keywords (Phase 1: Power Domains)
    POWER_DOMAIN_KW,   // power_domain
    SOURCES_KW,        // sources
    DISTRIBUTION_KW,   // distribution
    DECOUPLING_KW,     // decoupling
    NEAR_KW,           // near
    EACH_KW,           // each
    DISTRIBUTED_KW,    // distributed
    CONSTRAINTS_KW,    // constraints (already exists but listing for clarity)

    // Nodes (Grammar rules)
    SOURCE_FILE,    // Root node
    IMPORT_STMT,    // import ...;
    IMPORT_PATH,    // Ident.Ident...
    IMPORT_TARGET,  // Final IDENT or { ... } group
    IMPORT_TARGET_GROUP, // { Item, Item ... }
    ALIAS,          // `as AliasName` part of import
    BOARD_DEF,      // board BoardName { ... }
    ENTITY_DEF,     // entity EntityName { ... }
    COMPONENT_DEF,  // component ComponentName { ... }
    TYPEDEF_DEF,    // typedef TypeName [extends Base] { ... }
    TYPEDEF_BASE,   // Node wrapping the base type name after extends
    STRUCT_DEF,     // struct definition
    ENUM_DEF,       // enum definition
    ENUM_VARIANT,   // variant within an enum (e.g., Overcurrent, Fault(FaultKind))
    MATCH_EXPR,     // match expr { arms }
    MATCH_ARM,      // pattern => body
    MATCH_PATTERN,  // pattern in a match arm (literal, ident, path, wildcard)
    GENERIC_PARAMS, // <T: Type, V: voltage where V > 0> generic parameter block
    GENERIC_PARAM,  // Single generic parameter with optional type + constraints
    WHERE_CLAUSE,   // where V_IN >= 4.5V && V_IN <= 40V
    MEMBERSHIP_CONSTRAINT, // where channel in ("nmos", "pmos") — a param's allowed value set
    MODEL_IBIS_STMT, // ibis "file.ibs" component "NAME" [corner typ] [map { PIN = sig; }];
    TRAIT_DEF,      // trait TraitName { pin ...; const ...; }
    TRAIT_IMPL,     // impl TraitName for Component { ... }  or  component X impl Trait { ... }
    TRAIT_PIN,      // pin declaration within a trait
    TRAIT_CONST,    // const declaration within a trait
    TYPE_ARGS,      // <5V, 3.3V> type argument list on alias/instantiation
    TRAIT_BOUND,    // T: TraitName (in generic param list)
    INTERFACE_DEF,  // interface InterfaceName { ... }
    INTERFACE_SIGNAL,    // signal name: direction optional?;
    INTERFACE_REQUIREMENT, // require pullup(SDA, 4.7k);
    INTERFACE_PERSPECTIVE, // perspective master { ... }
    INTERFACE_INST,      // instance: InterfaceName();
    INTERFACE_FIELD_DECL, // interface [~]Name field;  (entity body)
    INTERFACE_FIELD_BINDINGS, // { SIG = PIN; SIG = PIN; ... } after a field decl
    INTERFACE_FIELD_BINDING,  // one entry of the above: `SIG = PIN`
    INTERFACE_WIRES_BLOCK,    // wires { dte.TX <-> dce.RX; ... }  (v0.7)
    INTERFACE_WIRE_MAPPING,   // one entry: `perspective.signal <-> perspective.signal`
    ENTITY_ALIASES_BLOCK,     // aliases { gpio0 = PB0; ... }       (v0.9)
    ENTITY_ALIAS_MAPPING,     // one entry: `alias_name = pin_name`
    CONSTRAINTS_BLOCK,        // constraints { stmt; stmt; ... }     (v0.8 timing)
    CONSTRAINT_STMT,          // one entry: `targets [-> targets]: prop_list;`
    CONSTRAINT_LHS,           // left-hand target list (raw tokens)
    CONSTRAINT_RHS,           // right-hand target list, only present for relations
    CONSTRAINT_PROPS,         // property list (raw tokens)
    TYPE_DEF,       // type TypeName = TypeExpression;
    PART_FAMILY_DEF,    // part_family Name : ClassPattern { require ...; attribute ...; }
    CLASS_PATTERN,      // : EntityName<*, "1%", "0603"> — the class skeleton
    REQUIRE_CLAUSE,     // require R in E96(1Ω, 10MΩ);
    STRUCT_LITERAL, // { field1: value1, field2: value2 }
    STRUCT_FIELD,   // field1: value1
    NULL_LITERAL,   // null
    
    // Testbench nodes
    TESTBENCH_DEF,       // testbench Name for Board { ... }
    SIMULATION_BLOCK,    // simulation { ... }
    SCOPE_DEF,          // scope "name" { ... }
    STIMULUS_BLOCK,     // stimulus { ... }
    VERIFY_BLOCK,       // verify { ... }
    MEASURE_BLOCK,      // measure { ... }
    ASSERTION,          // assert condition message;
    MEASUREMENT,        // name = expression;
    STIMULUS_ASSIGN,    // @signal: waveform;
    WAVEFORM_EXPR,      // ramp(...), sine(...), etc.
    CAPTURE_MODE,       // continuous, on_change(...), etc.
    TIME_SPEC,          // 10ms, 1us, etc.

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

    // Power domain nodes (Phase 1: Scalability)
    POWER_DOMAIN_DEF,       // power_domain @NAME = spec { ... }
    SOURCES_BLOCK,          // sources { ... }
    DISTRIBUTION_BLOCK,     // distribution { ... }
    DECOUPLING_BLOCK,       // decoupling { ... }
    DECOUPLING_RULE,        // near fpga: [caps] or distributed: [caps]
    SOURCE_DEFINITION,      // component: Type().pin;
    DISTRIBUTION_PIN_LIST,  // List of pins for distribution
    CAP_SPEC,               // 10µF @ 5 (capacitor specification)

    // Advanced pattern matching nodes (Phase 2: Advanced Patterns)
    PATTERN_KEYWORD,        // "even" or "odd" in brackets
    PATTERN_INDICES,        // Explicit list or stepped range
    
    // Individual statements for linear regulator syntax
    POWER_DECL,         // power VIN = 12V @ 2A;
    POWER_STAGE_CHAIN,  // |> protection |> filtering |> regulation (stage chain on power decl)
    STAGE_NAME,         // Single stage name node (wraps IDENT)
    GROUND_DECL,        // ground GND;
    CONNECTION_TARGET,  // U1.input or vin_rail

    // Items within Blocks
    CONST_DECL,        // const name: type = value;
    PARAM_DECL,        // const param_name: type = value;
    PORT_DECL,         // port_name: direction type ...;
    PIN_DECL,          // pin pin_name: type direction;
    COMPONENT_INST,    // Resistor R1 { ... }
    PARAM_ASSIGN_BLOCK,// The `(...)` or `{...}` part in component instantiation
    PARAM_ASSIGN,      // param_name = value (inside PARAM_ASSIGN_BLOCK)
    PARAM_PLACEHOLDER, // Empty params () or placeholder (?) for SPICE generation
    COMPONENT_TYPE,    // Type name used in component instantiation
    NET_DECL,          // net net_name[range]: type;
    NET_TYPE,          // The type keyword used in net decl (SIGNAL_KW etc)
    CONNECTION_STMT,   // PinRef -> PinRef; OR NetRef -> PinRef; OR PinRef -> NetRef; OR AssignStmt
    CONNECTION_CONSTRAINT, // where trace_length < 10mm, impedance = 50Ω
    CONSTRAINT_LIST,   // List of constraints after where
    CONSTRAINT_ITEM,   // Individual constraint (e.g., trace_length < 10mm)
    WITH_BLOCK,        // with routing(...) { connections }
    ASSIGN_STMT,       // assign NetRef = Expression;
    NET_REF,           // Reference to a net name
    PIN_REF,           // instance.pin[range] or net_name[range]
    PIN_METADATA,      // Pin metadata annotation @metadata(...)
    METADATA_PAIR,     // Key-value pair in metadata
    CONSTRAINT_TARGET, // Node for the target(s) in constrain()
    BUS_SUFFIX,        // [index] or [high:low]
    RANGE_EXPR,        // start to end
    VALUE,             // Generic value node (number, string, bool, etc.)
    TYPE_REF,          // Reference to a type (e.g., cmos_3v3 or signal(cmos_3v3))
    TYPE_SPECIFIER,    // (specifier_name)
    NULLABLE_TYPE,     // type?
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

    // Hierarchical entity support
    ENTITY_INST,       // Entity instantiation within an entity
    PORT_MAPPING,      // Single port mapping: PIN <- signal;
    ATTRIBUTE_DECL,    // Attribute declaration: attribute name = expression;
    WHEN_BLOCK,        // When block for behavioral conditions
    ATTRIBUTE_ASSIGNMENT, // Attribute assignment in when blocks
    SCOPED_ATTRIBUTE,  // Scoped attribute: attribute path.to.attr = value;
    ATTRIBUTE_PATH,    // Attribute path: path.to.attribute
    LEFT_ARROW,        // <- (new arrow for consistent port mapping)
    
    // Intent system nodes
    INTENT_CLAUSE,     // for intent_name(params)
    INTENT_CALL,       // intent_name(params)
    INTENT_PARAMS,     // (param1, param2: value, ...)
    INTENT_NAMED_PARAM,// name: value in intent parameters

    // Expansion block nodes (entity-level application circuit recipes)
    EXPANSION_BLOCK,       // expansion { ... }
    EXPANSION_INTERNAL_NET, // internal name: net;

    // Variant blocks (board-level SKU patches; v0.1 = DNP + value override)
    VARIANT_BLOCK,         // variant <Name> { ... }
    VARIANT_VALUE_OVERRIDE,// <instance>.value = <expr>;
    VARIANT_DNP_STMT,      // dnp <instance>;

    // Socket pairing (declared inside expansion blocks; marks one
    // child instance as being physically socketed in another, e.g.
    // a tube held in a chassis octal socket).
    EXPANSION_SOCKET_STMT, // socket <held> in <socket>;

    // Design block nodes (vendor-authored intent → bias values)
    DESIGN_BLOCK,          // design for <intent> { ... }
    DESIGN_REQUIRE_STMT,   // require <expr> else "<msg>";
    DESIGN_ASSIGNMENT,     // <child_name> = <expr>;
    DESIGN_INPUTS_DECL,    // inputs  { name; name; ... }       (Stage 5)
    DESIGN_OUTPUTS_DECL,   // outputs { name; name; ... }       (Stage 5)
    DESIGN_BODY_HOOK,      // body <lang> r#"..."#              (Stage 5)

    // Entity-level simulation block nodes (vendor device-simulation IP —
    // docs/spec/Vendor_Simulation_Blocks.md). Distinct from the testbench
    // SIMULATION_BLOCK (a sim *config* block). The stress surface (§4) is
    // built first; the model surface (§5) is reserved.
    SIM_BLOCK,             // simulation { stress { ... } model { ... } }
    STRESS_BLOCK,          // stress { const ...; <child>.<axis> = <expr>; }
    STRESS_ASSIGNMENT,     // <child_name>.<axis> = <expr>;
    MODEL_BLOCK,           // model { ... }   (§5 device-model surface)
    MODEL_NODE_STMT,       // node <net> <source|draws> = <expr>;
    CHECK_BLOCK,           // check { require <pred> else "MSG"; } (T2/ERC025)

    // Power-supply synthesis (docs/spec/Power_Supply_Synthesis.md §2):
    // supply @TARGET from @SOURCE { ripple_max: 30mV; using: TPS54331; ... }
    SUPPLY_STMT,           // the whole statement (board scope)
    SUPPLY_SPEC_ENTRY,     // one `key: value;` entry in the spec block

    // Symbol and layout definition nodes
    SYMBOL_DEF,            // symbol EntityName { ... }
    SYMBOL_BODY_HINT,      // body rectangle;
    SYMBOL_SIDE,           // left { pin1, pin2 } or right { group ... }
    SYMBOL_GROUP,          // group "label" { pin1, pin2 }
    SYMBOL_PART,           // part "label" { side ... } (Phase 2 stub)
    LAYOUT_DEF,            // layout EntityName { ... }
    LAYOUT_PACKAGE,        // package HTSSOP-16;
    LAYOUT_STACKUP,        // layer_stackup 4;
    LAYOUT_PLACE,          // place j_usb at (0, 20) rot 270;
    LAYOUT_OUTLINE,        // outline rect 80 50; | outline polygon (..) ..;
    LAYOUT_MOUNTING_HOLE,  // mounting_hole (5,5) drill 3.2 keepout 6;
    LAYOUT_KEEPOUT,        // keepout rect (60,40) (80,50);
    LAYOUT_CUTOUT,         // cutout rect (16,8) (22,12);

    // Placement block nodes (entity-level PCB placement recipes)
    PLACEMENT_BLOCK,       // placement { ... }
    PLACEMENT_ITEM,        // name at (x, y) rot deg;
    PLACEMENT_REFERENCE,   // reference "datasheet ref";

    // Safety compliance nodes
    SATISFIES_BLOCK,   // satisfies { ... }
    SATISFIES_ITEM,    // REQ_001: via component;
    SATISFIES_VIA,     // via component_name
    SATISFIES_DETAILS, // { field: value, ... }
    
    // Hierarchical requirement nodes
    SAFETY_GOAL_DEF,    // safety_goal Name(params) "title" { signal ..; effect ..; }  (library goal definition)
    SAFETY_ASSUMPTION_DEF, // safety_assumption Name(params) "text";  (library assumption-of-use definition)
    SAFETY_MISSION,     // mission { ambient = 55degC; on_hours = 8760; }  (board mission profile)
    SAFETY_MISSION_PHASE, // phase driving { time = 8%; ambient = 60degC; }  (one histogram phase)
    SAFETY_GOAL_PARAMS, // (vmax: voltage, level: asil = ASIL_B)
    SAFETY_SIGNAL_DECL, // signal RAIL: power;      (goal formal)
    SAFETY_EFFECT,      // effect name = expr severity S3;
    SAFETY_DEF,         // safety Name [of Entity] as ns { ... }
    SAFETY_LINK,        // `of Entity`
    SAFETY_NS,          // `as ns`
    SAFETY_GOAL_INST,   // SG: Goal(params) { formal: ns.h; ... } (kwargs);
    SAFETY_GOAL_INLINE, // goal SG: LEVEL "title" (kwargs) { effect ...; }
    SAFETY_BIND_ITEM,   // formal: ns.handle;
    SAFETY_MECHANISM,   // mechanism ns.h: psm(Goal, ...);
    SAFETY_FAULT,       // fault kind(targets) expect Goal.effect [detected_by ns.h] [within dur];
    SAFETY_WAIVE,       // waive ns.h qm "reason";
    SAFETY_ASSUME,      // assume Id(args) | Id "text";
    SAFETY_REFINES,     // ns.inst.Goal refines Goal;
    SAFETY_SATISFIED,   // ns.inst.Id satisfied_by ns.h;  |  ... waived "reason";
    SAFETY_DATA_BLOCK,  // entity-scope: safety { failure_state ..; seooc ..; terminal ..; handbook ..; assumption ..; }
    SAFETY_DATA_ITEM,   // one statement inside SAFETY_DATA_BLOCK (head ident + tokens up to ';')
    FAULT_INJECT_DEF,   // fault_inject short(a, b) -> verify { ... }
    SAFETY_ATTR,        // #[safety(...)] or #[safety_mechanism(...)]
    FUNCTIONAL_REQ_DEF, // functional_requirement FSR_001 { ... }
    TECHNICAL_REQ_DEF,  // technical_requirement TSR_001 { ... }
    REQ_PROPERTY,       // property: value pairs in requirements
    
    // Simulation and optimization nodes
    BEHAVIORAL_MODEL,       // @behavioral_model name { ... }
    OPTIMIZATION_STRATEGY,  // @optimization_strategy { ... }
    COMPONENT_KNOWLEDGE,    // @component_knowledge { ... }
    SIMULATION_REQUIREMENTS,// @simulation_requirements { ... }
    TEST_SEQUENCES,         // @test_sequences { ... }
    MODEL_SELECTOR,         // @model_selector { ... }
    MODEL_PROPERTY,         // property: value in behavioral model
    OPTIMIZATION_PHASE,     // phase definition in optimization strategy
    KNOWLEDGE_ITEM,         // item in component knowledge

    // Entity-scope power-domain contract (design-level, NOT safety):
    // `domain NAME pins=".." v=.. zmask=".." ...;` — the vendor's PDN
    // contract the board must meet whether or not it is a safety
    // product. The safety case consumes it via `assume pdn(...)`.
    DOMAIN_DECL,        // domain NAME k=v ...;

    // Board-scope decap synthesis from a domain's Z(f) mask:
    // `decouple <inst>.<domain> from "<lib.bhdl>" [max_parts=N];`
    // Real-part library, verified by AC impedance sweep vs the mask;
    // infeasibility is a hard error naming the physics.
    DECOUPLE_STMT,      // decouple x.D from "lib" k=v ...;

    // Entity partness declaration: `entity X as part|design { }` —
    // whether instantiation mints a physical self-part.
    ENTITY_KIND,        // as part | as design

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
        if raw.0 >= SyntaxKind::ERROR as u16 {
            SyntaxKind::ERROR
        } else {
            unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
        }
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