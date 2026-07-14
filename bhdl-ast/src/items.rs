// BHDL v2.0 AST Items
// Only supports v2.0 flow-based syntax

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken, HasName};
use rowan::ast::AstNode;
use crate::blocks::{LayerStackupBlock, DefaultDesignRulesBlock, ConstrainBlock};
use crate::common::{TypeRef, ParamAssign, ComponentInst, PortDecl, PinDecl};
use crate::v2_statements::ConnectionStmt;
use crate::expr::Expr;
use crate::v2_statements::{PowerDecl, GroundDecl, FlowStmt};
use crate::hierarchical::EntityInst;
use crate::attributes::AttributeDecl;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Board(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for Board {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::BOARD_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for Board {}

impl Board {
    // v2.0 boards have power/ground declarations and connection statements
    pub fn power_decls(&self) -> impl Iterator<Item = PowerDecl> {
        self.0.children().filter_map(PowerDecl::cast)
    }
    
    pub fn ground_decls(&self) -> impl Iterator<Item = GroundDecl> {
        self.0.children().filter_map(GroundDecl::cast)
    }
    
    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        self.0.children().filter_map(ConnectionStmt::cast)
    }
    
    pub fn flow_statements(&self) -> impl Iterator<Item = FlowStmt> {
        self.0.children().filter_map(FlowStmt::cast)
    }
    
    // Hierarchical entity support
    pub fn entity_instances(&self) -> impl Iterator<Item = EntityInst> {
        self.0.children().filter_map(EntityInst::cast)
    }

    pub fn component_instances(&self) -> impl Iterator<Item = ComponentInst> {
        self.0.children().filter_map(ComponentInst::cast)
    }

    pub fn layer_stackup_block(&self) -> Option<LayerStackupBlock> { 
        self.0.children().find_map(LayerStackupBlock::cast) 
    }
    
    pub fn default_design_rules_block(&self) -> Option<DefaultDesignRulesBlock> { 
        self.0.children().find_map(DefaultDesignRulesBlock::cast) 
    }
    
    pub fn constrain_block(&self) -> Option<ConstrainBlock> {
        self.0.children().find_map(ConstrainBlock::cast)
    }
    
    // Board metadata (attributes)
    pub fn attributes(&self) -> impl Iterator<Item = AttributeDecl> {
        self.0.children().filter_map(AttributeDecl::cast)
    }
    
    // Alias for consistency with test code
    pub fn attribute_decls(&self) -> impl Iterator<Item = AttributeDecl> {
        self.attributes()
    }
    
    // Behavioral modeling: when blocks
    pub fn when_blocks(&self) -> impl Iterator<Item = crate::behavioral::WhenBlock> {
        self.0.children().filter_map(crate::behavioral::WhenBlock::cast)
    }
    
    // v2.0 const declarations in boards
    pub fn const_decls(&self) -> impl Iterator<Item = ConstDecl> {
        self.0.children().filter_map(ConstDecl::cast)
    }

    // Phase 1: Power domains
    pub fn power_domains(&self) -> impl Iterator<Item = PowerDomain> {
        self.0.children().filter_map(PowerDomain::cast)
    }

    // Generate blocks for repetitive structures
    pub fn generate_blocks(&self) -> impl Iterator<Item = crate::blocks::GenerateBlock> {
        self.0.children().filter_map(crate::blocks::GenerateBlock::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Entity(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for Entity {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ENTITY_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for Entity {}

impl Entity {
    // v2.0 entities have pins and metadata
    pub fn pins(&self) -> impl Iterator<Item = PinDecl> {
        self.0.children().filter_map(PinDecl::cast)
    }

    // Keep ports() for compatibility with higher-level entities that might use ports
    pub fn ports(&self) -> impl Iterator<Item = PortDecl> {
        self.0.children().filter_map(PortDecl::cast)
    }

    pub fn param_list(&self) -> Option<ParamList> {
        self.0.children().find_map(ParamList::cast)
    }

    /// Get generic type parameters: `<T: Type, V: voltage, ...>`
    pub fn generic_params(&self) -> Option<GenericParams> {
        self.0.children().find_map(GenericParams::cast)
    }

    /// Get where clause constraints: `where V_IN >= 4.5V, V_OUT < V_IN`
    pub fn where_clause(&self) -> Option<WhereClause> {
        self.0.children().find_map(WhereClause::cast)
    }

    // Hierarchical entity support
    pub fn entity_instances(&self) -> impl Iterator<Item = EntityInst> {
        self.0.children().filter_map(EntityInst::cast)
    }

    pub fn component_instances(&self) -> impl Iterator<Item = ComponentInst> {
        self.0.children().filter_map(ComponentInst::cast)
    }

    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        self.0.children().filter_map(ConnectionStmt::cast)
    }

    // Entity metadata (attributes)
    pub fn attributes(&self) -> impl Iterator<Item = AttributeDecl> {
        self.0.children().filter_map(AttributeDecl::cast)
    }

    // Generate blocks for repetitive structures
    pub fn generate_blocks(&self) -> impl Iterator<Item = crate::blocks::GenerateBlock> {
        self.0.children().filter_map(crate::blocks::GenerateBlock::cast)
    }

    /// Get the expansion block, if present
    pub fn expansion_block(&self) -> Option<ExpansionBlock> {
        self.0.children().find_map(ExpansionBlock::cast)
    }

    /// Get the placement block, if present
    pub fn placement_block(&self) -> Option<PlacementBlock> {
        self.0.children().find_map(PlacementBlock::cast)
    }
}

/// Expansion block inside an entity definition.
/// Contains flow statements, internal net declarations, and component instantiations
/// that describe the entity's "typical application circuit".
///
/// ```bhdl
/// entity BuckRegulator<V_OUT: voltage>(...) {
///     ...
///     expansion {
///         internal sw: net;
///         VOUT -> L: Ind(l_value).1 -> L.2 -> sw;
///         sw -> D: Diode(diode_vf).K; D.A -> GND;
///         VOUT -> C_out: Cap(c_out).1; C_out.2 -> GND;
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExpansionBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ExpansionBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::EXPANSION_BLOCK }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ExpansionBlock {
    /// Get all connection statements in the expansion block
    pub fn connection_stmts(&self) -> impl Iterator<Item = ConnectionStmt> {
        self.0.children().filter_map(ConnectionStmt::cast)
    }

    /// Get all internal net declarations
    pub fn internal_nets(&self) -> Vec<String> {
        self.0.children()
            .filter(|n| n.kind() == SyntaxKind::EXPANSION_INTERNAL_NET)
            .filter_map(|n| {
                n.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() == SyntaxKind::IDENT)
                    .map(|t| t.text().to_string())
            })
            .collect()
    }

    /// Get the raw text of the expansion block body (for debugging)
    pub fn body_text(&self) -> String {
        self.0.text().to_string()
    }
}

/// Placement block inside an entity definition.
/// Contains placement positions for child components from expansion blocks,
/// typically referencing datasheet recommended layouts.
///
/// ```bhdl
/// entity BuckRegulator(...) {
///     ...
///     placement {
///         reference "AP63205 Datasheet Fig.5";
///         L_out at (-0.5, -5.0) rot 0;
///         C_out at (5.5, -2.0) rot 90;
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlacementBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for PlacementBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PLACEMENT_BLOCK }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl PlacementBlock {
    /// Get all placement items in the block
    pub fn placement_items(&self) -> Vec<PlacementItem> {
        self.0.children()
            .filter_map(PlacementItem::cast)
            .collect()
    }

    /// Get the reference text (e.g., datasheet figure reference)
    pub fn reference_text(&self) -> Option<String> {
        self.0.children()
            .find(|n| n.kind() == SyntaxKind::PLACEMENT_REFERENCE)
            .and_then(|n| n.children_with_tokens()
                .find_map(|t| if t.kind() == SyntaxKind::STRING {
                    Some(t.as_token()?.text().trim_matches('"').to_string())
                } else { None }))
    }
}

/// A single placement item: `NAME at (x, y) rot deg;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlacementItem(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for PlacementItem {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PLACEMENT_ITEM }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl PlacementItem {
    /// Get the component name
    pub fn component_name(&self) -> Option<String> {
        self.0.children_with_tokens()
            .find_map(|t| if t.kind() == SyntaxKind::IDENT {
                Some(t.as_token()?.text().to_string())
            } else { None })
    }

    /// Extract x and y coordinates from the placement item.
    /// Handles negative numbers by looking for MINUS + NUMBER pairs via expression nodes.
    pub fn coordinates(&self) -> Option<(f64, f64)> {
        let numbers = self.collect_signed_numbers();
        if numbers.len() >= 2 { Some((numbers[0], numbers[1])) } else { None }
    }

    /// Extract rotation in degrees (the third number, if present after `rot`)
    pub fn rotation_deg(&self) -> Option<f64> {
        let numbers = self.collect_signed_numbers();
        if numbers.len() >= 3 { Some(numbers[2]) } else { None }
    }

    /// Collect all signed numbers from descendant tokens.
    /// Handles PREFIX_EXPR with MINUS for negative numbers.
    fn collect_signed_numbers(&self) -> Vec<f64> {
        let mut numbers = Vec::new();
        // Walk all descendant nodes/tokens to find numbers
        // PREFIX_EXPR nodes contain a MINUS followed by a NUMBER
        // Regular NUMBER tokens are positive values
        self.collect_numbers_recursive(&self.0, &mut numbers);
        numbers
    }

    fn collect_numbers_recursive(&self, node: &SyntaxNode<BhdlLanguage>, numbers: &mut Vec<f64>) {
        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    if n.kind() == SyntaxKind::PREFIX_EXPR {
                        // Check if this is a negation: MINUS NUMBER
                        let has_minus = n.children_with_tokens()
                            .any(|t| t.kind() == SyntaxKind::MINUS);
                        let num_val = n.descendants_with_tokens()
                            .filter_map(|t| t.into_token())
                            .find(|t| t.kind() == SyntaxKind::NUMBER)
                            .and_then(|t| t.text().parse::<f64>().ok());
                        if let Some(val) = num_val {
                            numbers.push(if has_minus { -val } else { val });
                        }
                    } else if n.kind() == SyntaxKind::EXPRESSION {
                        // Recurse into EXPRESSION nodes
                        self.collect_numbers_recursive(&n, numbers);
                    } else {
                        // For other node types, also recurse
                        self.collect_numbers_recursive(&n, numbers);
                    }
                }
                rowan::NodeOrToken::Token(t) => {
                    if t.kind() == SyntaxKind::NUMBER {
                        // Direct NUMBER token (not inside PREFIX_EXPR)
                        // Check parent is not PREFIX_EXPR (already handled)
                        let parent_is_prefix = node.kind() == SyntaxKind::PREFIX_EXPR;
                        if !parent_is_prefix {
                            if let Ok(val) = t.text().parse::<f64>() {
                                numbers.push(val);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Generic type parameter list: `<T: Type, V: voltage, ...>`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParams(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for GenericParams {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::GENERIC_PARAMS }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl GenericParams {
    /// Iterate over individual generic parameter declarations
    pub fn params(&self) -> impl Iterator<Item = GenericParamNode> {
        self.0.children().filter_map(GenericParamNode::cast)
    }
}

/// A single generic parameter: `T`, `T: BoundType`, or `T: BoundType = default`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParamNode(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for GenericParamNode {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::GENERIC_PARAM }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl GenericParamNode {
    /// Get the parameter name (e.g., "V_OUT", "T")
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }

    /// Get the type bound name (e.g., "voltage", "Passive")
    /// This is the IDENT after the colon, if present.
    pub fn type_bound(&self) -> Option<String> {
        let mut found_colon = false;
        for element in self.0.children_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::COLON => { found_colon = true; }
                    SyntaxKind::IDENT if found_colon => {
                        return Some(token.text().to_string());
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Get the default value expression, if present (after `=`)
    pub fn default_value(&self) -> Option<Expr> {
        let mut found_eq = false;
        for element in self.0.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) if token.kind() == SyntaxKind::EQ => {
                    found_eq = true;
                }
                rowan::NodeOrToken::Node(node) if found_eq => {
                    return Expr::cast(node);
                }
                _ => {}
            }
        }
        None
    }
}

/// Where clause: `where V_IN >= 4.5V, V_OUT < V_IN`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhereClause(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for WhereClause {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::WHERE_CLAUSE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

/// Type argument list on alias or instantiation: `<5V, 3.3V>`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeArgs(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for TypeArgs {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TYPE_ARGS }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl TypeArgs {
    /// Get the type argument expressions
    pub fn args(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ComponentDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::COMPONENT_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for ComponentDef {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfaceDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for InterfaceDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::INTERFACE_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for InterfaceDef {}

impl InterfaceDef {
    pub fn params(&self) -> Option<ParamList> {
        self.0.children().find_map(ParamList::cast)
    }
    
    pub fn signals(&self) -> impl Iterator<Item = crate::interfaces::InterfaceSignal> {
        self.0.children().filter_map(crate::interfaces::InterfaceSignal::cast)
    }
    
    pub fn requirements(&self) -> impl Iterator<Item = crate::interfaces::InterfaceRequirement> {
        self.0.children().filter_map(crate::interfaces::InterfaceRequirement::cast)
    }
    
    pub fn perspectives(&self) -> impl Iterator<Item = crate::interfaces::InterfacePerspective> {
        self.0.children().filter_map(crate::interfaces::InterfacePerspective::cast)
    }
    
    pub fn nested_interfaces(&self) -> impl Iterator<Item = InterfaceDef> {
        self.0.children().filter_map(InterfaceDef::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypedefDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for TypedefDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TYPEDEF_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for TypedefDef {}

impl TypedefDef {
    pub fn base_type(&self) -> Option<TypedefBase> {
        self.0.children().find_map(TypedefBase::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypedefBase(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for TypedefBase {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TYPEDEF_BASE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamList(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ParamList {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PARAM_LIST }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ParamList {
    pub fn params(&self) -> impl Iterator<Item = ParamAssign> {
        self.0.children().filter_map(ParamAssign::cast)
    }
    
    /// Get entity parameter definitions (name: type = default)
    pub fn param_defs(&self) -> impl Iterator<Item = EntityParam> {
        self.0.children().filter_map(EntityParam::cast)
    }

    /// Check if this is an entity parameter list (has type annotations)
    pub fn is_entity_params(&self) -> bool {
        // Entity params have COLON tokens for type annotations
        self.0.children_with_tokens()
            .any(|element| element.as_token()
                .map(|token| token.kind() == SyntaxKind::COLON)
                .unwrap_or(false))
    }
}

/// Entity parameter definition: name: type = default_value
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityParam(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for EntityParam {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PARAM_DECL
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self(syntax))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl EntityParam {
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
    
    pub fn type_ref(&self) -> Option<TypeRef> {
        self.0.children().find_map(TypeRef::cast)
    }
    
    pub fn default_value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

/// Represents an import statement
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportStmt(SyntaxNode<BhdlLanguage>);

impl AstNode for ImportStmt {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IMPORT_STMT
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self(syntax))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl ImportStmt {
    /// Get the path of the import (e.g., "../../../bhdl-stdlib/components/power/TPS54331.bhdl")
    pub fn path(&self) -> Option<String> {
        // Look for IMPORT_PATH node first
        for child in self.0.children() {
            if child.kind() == SyntaxKind::IMPORT_PATH {
                // Check if it contains a STRING token (destructuring imports)
                if let Some(string_token) = child.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() == SyntaxKind::STRING)
                {
                    let text = string_token.text();
                    return Some(text.trim_matches('"').to_string());
                }
            }
        }
        
        // Fallback: look for STRING anywhere in the import (for compatibility)
        self.0.descendants()
            .filter(|n| n.kind() == SyntaxKind::STRING)
            .next()
            .and_then(|n| n.first_token())
            .map(|t| {
                let text = t.text();
                // Remove quotes from string literal
                text.trim_matches('"').to_string()
            })
    }
    
    /// Get the imported names for destructuring imports { Name1, Name2 }
    pub fn imported_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        
        // Look for IMPORT_TARGET_GROUP
        for child in self.0.children() {
            if child.kind() == SyntaxKind::IMPORT_TARGET_GROUP {
                // Look for IMPORT_TARGET nodes within the group
                for target in child.children() {
                    if target.kind() == SyntaxKind::IMPORT_TARGET {
                        // Get the IDENT from the target
                        if let Some(ident) = target.children_with_tokens()
                            .filter_map(|e| e.into_token())
                            .find(|t| t.kind() == SyntaxKind::IDENT)
                        {
                            names.push(ident.text().to_string());
                        }
                    }
                }
                
                // Also collect direct IDENTs (for backwards compatibility)
                for token in child.children_with_tokens() {
                    if let Some(t) = token.into_token() {
                        if t.kind() == SyntaxKind::IDENT {
                            // Only add if not already in the list
                            let text = t.text().to_string();
                            if !names.contains(&text) {
                                names.push(text);
                            }
                        }
                    }
                }
            }
        }
        
        names
    }
}

// --- Const Declaration --- `const name: type = value;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstDecl(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ConstDecl {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::CONST_DECL }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for ConstDecl {}

impl ConstDecl {
    /// Get the const keyword
    pub fn const_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::CONST_KW)
    }
    
    /// Get the type annotation (if present)
    pub fn type_ref(&self) -> Option<TypeRef> {
        self.0.children().find_map(TypeRef::cast)
    }
    
    /// Get the initializer expression
    pub fn initializer(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    
    /// Get the colon token
    pub fn colon_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::COLON)
    }
    
    /// Get the equals token
    pub fn eq_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::EQ)
    }
}

// =================================
// Phase 1: Power Domain AST Nodes
// =================================

/// Power domain definition: `power_domain @VCC_3V3 = 3.3V @ 10A { ... }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PowerDomain(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for PowerDomain {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::POWER_DOMAIN_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl PowerDomain {
    /// Get the net name (without @ prefix)
    pub fn net_name(&self) -> Option<String> {
        // Find the IDENT token after the @ token
        let mut found_at = false;
        for element in self.0.children_with_tokens() {
            if let Some(token) = element.as_token() {
                if token.kind() == SyntaxKind::AT {
                    found_at = true;
                } else if found_at && token.kind() == SyntaxKind::IDENT {
                    return Some(token.text().to_string());
                }
            }
        }
        None
    }

    /// Get the voltage specification
    pub fn voltage(&self) -> Option<Expr> {
        // First expression after = is the voltage
        let mut found_eq = false;
        for child in self.0.children() {
            if child.kind() == SyntaxKind::EQ {
                found_eq = true;
            } else if found_eq {
                if let Some(expr) = Expr::cast(child) {
                    return Some(expr);
                }
            }
        }
        None
    }

    /// Get the current specification
    pub fn current(&self) -> Option<Expr> {
        // Second expression after = and @ is the current
        let mut expressions = self.0.children()
            .filter_map(|child| Expr::cast(child));

        // Skip voltage (first expression)
        expressions.next();
        // Return current (second expression)
        expressions.next()
    }

    /// Get the sources block
    pub fn sources_block(&self) -> Option<SourcesBlock> {
        self.0.children().find_map(SourcesBlock::cast)
    }

    /// Get the distribution block
    pub fn distribution_block(&self) -> Option<DistributionBlock> {
        self.0.children().find_map(DistributionBlock::cast)
    }

    /// Get the decoupling block
    pub fn decoupling_block(&self) -> Option<DecouplingBlock> {
        self.0.children().find_map(DecouplingBlock::cast)
    }

    /// Get constraints as key-value pairs
    pub fn constraints(&self) -> Vec<(String, Expr)> {
        // TODO: Implement constraint parsing when needed
        Vec::new()
    }
}

/// Sources block: `sources { reg1: LDO_3V3().OUT; }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourcesBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for SourcesBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::SOURCES_BLOCK }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl SourcesBlock {
    /// Get all source definitions
    pub fn sources(&self) -> impl Iterator<Item = SourceDefinition> {
        self.0.children().filter_map(SourceDefinition::cast)
    }
}

/// Source definition: `handle: Component().pin;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceDefinition(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for SourceDefinition {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::SOURCE_DEFINITION }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl SourceDefinition {
    /// Get the handle name
    pub fn handle(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }

    /// Get the component type name
    pub fn component_type(&self) -> Option<String> {
        // Find the second IDENT (after the handle)
        let idents: Vec<_> = self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT)
            .collect();

        idents.get(1).map(|t| t.text().to_string())
    }

    /// Get the pin name
    pub fn pin_name(&self) -> Option<String> {
        // Find the last IDENT (after the dot)
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT)
            .last()
            .map(|t| t.text().to_string())
    }
}

/// Distribution block: `distribution { fpga.VCCO[0..7]; ics[*].VDD; }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DistributionBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for DistributionBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::DISTRIBUTION_BLOCK }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl DistributionBlock {
    /// Get all pin lists
    pub fn pin_lists(&self) -> impl Iterator<Item = DistributionPinList> {
        self.0.children().filter_map(DistributionPinList::cast)
    }
}

/// Distribution pin list: `fpga.VCCO[0..7];` or `ics[*].VDD;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DistributionPinList(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for DistributionPinList {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::DISTRIBUTION_PIN_LIST }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl DistributionPinList {
    /// Get the component reference (first identifier)
    pub fn component(&self) -> Option<String> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
    }

    /// Get the pin name: the token after the last dot outside brackets.
    /// Pins may be named (`fpga.VCCAUX`) or numeric (`resistor[*].1`) —
    /// a NUMBER after the dot is a pin name too, not an index.
    pub fn pin_name(&self) -> Option<String> {
        let tokens: Vec<_> = self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .collect();
        let mut depth = 0i32;
        let mut last_dot = None;
        for (i, t) in tokens.iter().enumerate() {
            match t.kind() {
                SyntaxKind::L_BRACKET => depth += 1,
                SyntaxKind::R_BRACKET => depth -= 1,
                SyntaxKind::DOT if depth == 0 => last_dot = Some(i),
                _ => {}
            }
        }
        tokens[last_dot? + 1..].iter()
            .find(|t| matches!(t.kind(), SyntaxKind::IDENT | SyntaxKind::NUMBER))
            .map(|t| t.text().to_string())
    }

    /// Check if this uses wildcard selector [*]
    pub fn has_wildcard(&self) -> bool {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .any(|t| t.kind() == SyntaxKind::STAR)
    }

    /// Get range expressions (if present)
    pub fn ranges(&self) -> Vec<Expr> {
        self.0.children().filter_map(Expr::cast).collect()
    }

    /// Get all path segments with their modifiers (Phase 3: Hierarchical Wildcards)
    /// Returns segments like: ["sensor_board[*]", "sensor", "VCC"]
    /// This reconstructs the path from the parsed tokens including wildcards and arrays
    pub fn path_segments(&self) -> Vec<String> {
        let mut segments = Vec::new();
        let mut current_segment = String::new();
        let mut in_brackets = false;
        let mut saw_dot_before_star = false;

        // Iterate through all tokens to reconstruct segments
        for element in self.0.children_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::DOT => {
                        if !in_brackets {
                            // DOT outside brackets means segment boundary
                            // Complete current segment and prepare for next
                            if !current_segment.is_empty() {
                                segments.push(current_segment.clone());
                                current_segment.clear();
                            }
                            saw_dot_before_star = true;
                        }
                    }
                    SyntaxKind::IDENT => {
                        current_segment.push_str(token.text());
                        saw_dot_before_star = false;
                    }
                    SyntaxKind::STAR => {
                        if in_brackets {
                            // Wildcard inside brackets: [*]
                            current_segment.push('*');
                        } else if saw_dot_before_star {
                            // Bare wildcard after dot: .*sensor
                            current_segment.push('*');
                        } else {
                            current_segment.push('*');
                        }
                        saw_dot_before_star = false;
                    }
                    SyntaxKind::L_BRACKET => {
                        current_segment.push('[');
                        in_brackets = true;
                        saw_dot_before_star = false;
                    }
                    SyntaxKind::R_BRACKET => {
                        current_segment.push(']');
                        in_brackets = false;
                        saw_dot_before_star = false;
                    }
                    SyntaxKind::DOT_DOT => {
                        if in_brackets {
                            current_segment.push_str("..");
                        }
                        saw_dot_before_star = false;
                    }
                    SyntaxKind::NUMBER => {
                        // Inside brackets: an index/range bound. Outside:
                        // a numeric pin segment (`resistor[*].1`).
                        current_segment.push_str(token.text());
                        saw_dot_before_star = false;
                    }
                    SyntaxKind::SEMI => {
                        // End of pin list
                        break;
                    }
                    _ => {
                        saw_dot_before_star = false;
                    }
                }
            }
        }

        // Add the last segment
        if !current_segment.is_empty() {
            segments.push(current_segment);
        }

        segments
    }

    /// Check if this is a hierarchical path (more than 2 segments)
    /// Examples:
    ///   fpga.VCC -> false (2 segments: component.pin)
    ///   sensor_board[*].sensor.VCC -> true (3 segments: entity.component.pin)
    pub fn is_hierarchical(&self) -> bool {
        self.path_segments().len() > 2
    }

    /// Get the full hierarchical path as a string
    /// Examples:
    ///   "fpga.VCCO" -> "fpga.VCCO"
    ///   "sensor_board[*].sensor.VCC" -> "sensor_board[*].sensor.VCC"
    pub fn full_path(&self) -> String {
        self.path_segments().join(".")
    }

    /// Get the pattern type from bracket notation (Phase 2: Advanced Patterns)
    /// Analyzes the AST to determine what kind of pattern is being used
    pub fn pattern_type(&self) -> PatternType {
        // Look for bracket contents in the syntax tree
        for element in self.0.descendants_with_tokens() {
            if let Some(node) = element.as_node() {
                match node.kind() {
                    SyntaxKind::PATTERN_KEYWORD => {
                        // Get the keyword text
                        if let Some(token) = node.first_token() {
                            match token.text() {
                                "even" => return PatternType::EvenKeyword,
                                "odd" => return PatternType::OddKeyword,
                                _ => {}
                            }
                        }
                    }
                    SyntaxKind::PATTERN_INDICES => {
                        // Analyze the pattern indices structure
                        return self.parse_pattern_indices(node);
                    }
                    _ => {}
                }
            } else if let Some(token) = element.as_token() {
                // Check for simple wildcard [*]
                if token.kind() == SyntaxKind::STAR {
                    // Make sure it's inside brackets (not a bare wildcard prefix)
                    let parent = token.parent();
                    if parent.map(|p| p.kind()) == Some(SyntaxKind::DISTRIBUTION_PIN_LIST) {
                        return PatternType::Wildcard;
                    }
                }
            }
        }

        // No pattern found, default to wildcard
        PatternType::Wildcard
    }

    /// Parse PATTERN_INDICES node to determine specific pattern type
    fn parse_pattern_indices(&self, node: &SyntaxNode<BhdlLanguage>) -> PatternType {
        let expressions: Vec<Expr> = node.children().filter_map(Expr::cast).collect();

        if expressions.is_empty() {
            return PatternType::Wildcard;
        }

        // Count dots and colons to determine pattern type
        let has_dot_dot = node.children_with_tokens()
            .any(|e| e.as_token().map(|t| t.kind() == SyntaxKind::DOT_DOT).unwrap_or(false));
        let has_colon = node.children_with_tokens()
            .any(|e| e.as_token().map(|t| t.kind() == SyntaxKind::COLON).unwrap_or(false));
        let has_comma = node.children_with_tokens()
            .any(|e| e.as_token().map(|t| t.kind() == SyntaxKind::COMMA).unwrap_or(false));

        if has_dot_dot {
            // Range pattern: [0..7] or [0..7:2]
            if expressions.len() >= 2 {
                let start = self.extract_number_from_expr(&expressions[0]).unwrap_or(0);
                let end = self.extract_number_from_expr(&expressions[1]).unwrap_or(0);

                if has_colon && expressions.len() >= 3 {
                    // Stepped range: [0..7:2]
                    let step = self.extract_number_from_expr(&expressions[2]).unwrap_or(1);
                    return PatternType::SteppedRange(start, end, step);
                } else {
                    // Simple range: [0..7]
                    return PatternType::SimpleRange(start, end);
                }
            }
        } else if has_comma {
            // Explicit list: [0,2,4,8]
            let indices: Vec<i32> = expressions.iter()
                .filter_map(|e| self.extract_number_from_expr(e))
                .collect();
            return PatternType::ExplicitList(indices);
        } else if expressions.len() == 1 {
            // Single index: [5] - treat as explicit list with one element
            if let Some(index) = self.extract_number_from_expr(&expressions[0]) {
                return PatternType::ExplicitList(vec![index]);
            }
        }

        PatternType::Wildcard
    }

    /// Extract integer value from expression
    fn extract_number_from_expr(&self, expr: &Expr) -> Option<i32> {
        let text = expr.syntax().text().to_string();
        text.trim().parse().ok()
    }

    /// Get pattern parameters with pre-computed indices
    pub fn pattern_params(&self) -> PatternParams {
        let pattern_type = self.pattern_type();
        let indices = match &pattern_type {
            PatternType::SimpleRange(start, end) => {
                (*start..=*end).collect()
            }
            PatternType::SteppedRange(start, end, step) => {
                let mut result = Vec::new();
                let mut i = *start;
                while i <= *end {
                    result.push(i);
                    i += step;
                }
                result
            }
            PatternType::ExplicitList(list) => list.clone(),
            _ => Vec::new(), // Wildcard, Even, Odd don't have pre-computed indices
        };

        PatternParams {
            pattern_type,
            indices,
        }
    }
}

/// Pattern type classification (Phase 2: Advanced Patterns)
#[derive(Debug, Clone, PartialEq)]
pub enum PatternType {
    Wildcard,                    // [*]
    SimpleRange(i32, i32),       // [0..7]
    SteppedRange(i32, i32, i32), // [0..15:2]
    ExplicitList(Vec<i32>),      // [0,2,4,8]
    EvenKeyword,                 // [even]
    OddKeyword,                  // [odd]
}

/// Pattern parameters extracted from AST
#[derive(Debug, Clone)]
pub struct PatternParams {
    pub pattern_type: PatternType,
    pub indices: Vec<i32>, // Pre-computed indices for ranges and lists
}


/// Decoupling block: `decoupling { near fpga: [10µF @ 5]; distributed: [0.1µF @ 50]; }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecouplingBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for DecouplingBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::DECOUPLING_BLOCK }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl DecouplingBlock {
    /// Get all decoupling rules
    pub fn rules(&self) -> impl Iterator<Item = DecouplingRule> {
        self.0.children().filter_map(DecouplingRule::cast)
    }
}

/// Decoupling rule: `near fpga: [10µF @ 5, 1µF @ 10];` or `distributed: [0.1µF @ 50];`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecouplingRule(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for DecouplingRule {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::DECOUPLING_RULE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl DecouplingRule {
    /// Check if this is a "near" placement
    pub fn is_near(&self) -> bool {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .any(|t| t.kind() == SyntaxKind::NEAR_KW)
    }

    /// Check if this is a "distributed" placement
    pub fn is_distributed(&self) -> bool {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .any(|t| t.kind() == SyntaxKind::DISTRIBUTED_KW)
    }

    /// Get the component reference (for "near" rules)
    pub fn near_component(&self) -> Option<String> {
        if !self.is_near() {
            return None;
        }

        // Find IDENT after NEAR_KW
        let mut found_near = false;
        for element in self.0.children_with_tokens() {
            if let Some(token) = element.as_token() {
                if token.kind() == SyntaxKind::NEAR_KW {
                    found_near = true;
                } else if found_near && token.kind() == SyntaxKind::IDENT {
                    return Some(token.text().to_string());
                }
            }
        }
        None
    }

    /// Check if this uses "each" modifier
    pub fn has_each(&self) -> bool {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .any(|t| t.kind() == SyntaxKind::EACH_KW)
    }

    /// Get all capacitor specifications
    pub fn cap_specs(&self) -> impl Iterator<Item = CapSpec> {
        self.0.children().filter_map(CapSpec::cast)
    }
}

/// Capacitor specification: `10µF @ 5` (value @ count)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapSpec(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for CapSpec {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::CAP_SPEC }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl CapSpec {
    /// Get the capacitance value expression
    pub fn value(&self) -> Option<Expr> {
        // First expression is the value
        self.0.children().find_map(Expr::cast)
    }

    /// Get the count expression
    pub fn count(&self) -> Option<Expr> {
        // Second expression is the count
        let mut exprs = self.0.children().filter_map(Expr::cast);
        exprs.next(); // Skip value
        exprs.next()  // Return count
    }
}

// =================================
// Symbol and Layout Definitions
// =================================

/// Symbol definition: `symbol EntityName { body rectangle; left { VIN, EN } ... }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for SymbolDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::SYMBOL_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for SymbolDef {}

impl SymbolDef {
    /// Get the body hint (e.g., "rectangle", "triangle")
    pub fn body_hint(&self) -> Option<String> {
        self.0.children()
            .find(|n| n.kind() == SyntaxKind::SYMBOL_BODY_HINT)
            .and_then(|n| {
                n.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() == SyntaxKind::IDENT)
                    .map(|t| t.text().to_string())
            })
    }

    /// Get all side definitions
    pub fn sides(&self) -> impl Iterator<Item = SymbolSide> {
        self.0.children().filter_map(SymbolSide::cast)
    }

    /// Get all part definitions (Phase 2 multi-part symbols)
    pub fn parts(&self) -> impl Iterator<Item = SymbolPart> {
        self.0.children().filter_map(SymbolPart::cast)
    }
}

/// A side in a symbol definition: `left { VIN, EN }` or `left { group "Power" { ... } }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolSide(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for SymbolSide {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::SYMBOL_SIDE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl SymbolSide {
    /// Get the side keyword text: "left", "right", "top", "bottom"
    pub fn side(&self) -> Option<String> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| matches!(t.kind(),
                SyntaxKind::LEFT_KW | SyntaxKind::RIGHT_KW |
                SyntaxKind::TOP_KW | SyntaxKind::BOTTOM_KW))
            .map(|t| t.text().to_string())
    }

    /// Get bare pin names (not inside groups)
    pub fn pins(&self) -> Vec<String> {
        let mut pins = Vec::new();
        let mut in_group = false;
        for element in self.0.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Node(ref n) if n.kind() == SyntaxKind::SYMBOL_GROUP => {
                    in_group = true;
                }
                rowan::NodeOrToken::Token(ref t) if t.kind() == SyntaxKind::IDENT && !in_group => {
                    pins.push(t.text().to_string());
                }
                rowan::NodeOrToken::Token(ref t) if t.kind() == SyntaxKind::NUMBER && !in_group => {
                    pins.push(t.text().to_string());
                }
                _ => {}
            }
            // Reset in_group after processing a group node
            if let rowan::NodeOrToken::Node(ref n) = element {
                if n.kind() == SyntaxKind::SYMBOL_GROUP {
                    in_group = false;
                }
            }
        }
        pins
    }

    /// Get all pin names on this side (including those inside groups)
    pub fn all_pins(&self) -> Vec<String> {
        let mut pins = self.pins();
        for group in self.groups() {
            pins.extend(group.pins());
        }
        pins
    }

    /// Get group definitions on this side
    pub fn groups(&self) -> impl Iterator<Item = SymbolGroup> {
        self.0.children().filter_map(SymbolGroup::cast)
    }
}

/// A group within a symbol side: `group "Power" { VDD, VDDA, VBAT }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolGroup(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for SymbolGroup {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::SYMBOL_GROUP }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl SymbolGroup {
    /// Get the group label (e.g., "Power", "SPI1")
    pub fn label(&self) -> Option<String> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::STRING)
            .map(|t| t.text().trim_matches('"').to_string())
    }

    /// Get pin names in this group
    pub fn pins(&self) -> Vec<String> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT || t.kind() == SyntaxKind::NUMBER)
            .map(|t| t.text().to_string())
            .collect()
    }
}

/// A part in a multi-part symbol: `part "Power" { left { ... } bottom { ... } }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolPart(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for SymbolPart {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::SYMBOL_PART }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl SymbolPart {
    /// Get the part label
    pub fn label(&self) -> Option<String> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::STRING)
            .map(|t| t.text().trim_matches('"').to_string())
    }

    /// Get sides within this part
    pub fn sides(&self) -> impl Iterator<Item = SymbolSide> {
        self.0.children().filter_map(SymbolSide::cast)
    }
}

/// Layout definition: `layout EntityName { package HTSSOP-16; }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayoutDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for LayoutDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::LAYOUT_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for LayoutDef {}

impl LayoutDef {
    /// Get the package name (e.g., "HTSSOP-16", "QFN-48")
    pub fn package(&self) -> Option<String> {
        self.0.children()
            .find(|n| n.kind() == SyntaxKind::LAYOUT_PACKAGE)
            .map(|n| {
                // Collect all tokens after PACKAGE_KW and before SEMI
                let mut found_pkg = false;
                let mut name_parts = Vec::new();
                for element in n.children_with_tokens() {
                    if let Some(token) = element.as_token() {
                        if token.kind() == SyntaxKind::PACKAGE_KW {
                            found_pkg = true;
                        } else if token.kind() == SyntaxKind::SEMI {
                            break;
                        } else if found_pkg && token.kind() != SyntaxKind::WHITESPACE {
                            name_parts.push(token.text().to_string());
                        }
                    }
                }
                name_parts.join("")
            })
    }

    /// Board-level layer count from `layer_stackup N;`.
    pub fn layer_stackup(&self) -> Option<usize> {
        self.0
            .children()
            .find(|n| n.kind() == SyntaxKind::LAYOUT_STACKUP)
            .and_then(|n| {
                n.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find_map(|t| t.text().parse::<usize>().ok())
            })
    }
}
// ─── part_family declarations (v0.2 catalog) ───────────────────────
//
// Catalog row binding a manufacturer's product family to one or more
// classes (bound-generic entity tuples). See
// docs/spec/Parameterization_And_BOM_Resolution.md §3–§4.
//
//   part_family TI_LM317T : LM317 {
//       attribute mpn = "LM317T";
//   }
//
//   part_family Yageo_RC0603FR_07 : Resistor<R: *, "1%", "0603"> {
//       require R in E96(1Ω, 10MΩ);
//       attribute mpn_template = "RC0603FR-07{e96_code(R)}L";
//   }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PartFamilyDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for PartFamilyDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PART_FAMILY_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for PartFamilyDef {}

impl PartFamilyDef {
    /// The class-pattern child node: `: EntityName<args>` after the
    /// family name. Used by the catalog scan to find the entity this
    /// family populates.
    pub fn class_pattern(&self) -> Option<ClassPattern> {
        self.0.children().find_map(ClassPattern::cast)
    }

    /// Iterator over `require ...;` constraint clauses in the body.
    pub fn require_clauses(&self) -> impl Iterator<Item = RequireClause> {
        self.0.children().filter_map(RequireClause::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClassPattern(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ClassPattern {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::CLASS_PATTERN }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ClassPattern {
    /// The entity name this family populates (e.g. "Resistor",
    /// "AP2112K", "LM317"). First IDENT after the `:` token.
    pub fn entity_name(&self) -> Option<String> {
        self.0
            .children_with_tokens()
            .filter_map(|el| el.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
    }

    /// Optional generic-args block (`<R: *, "1%", "0603">`). Returns
    /// the raw TYPE_ARGS node — the catalog scan re-walks it to bind
    /// wildcards and literal pins against an instance's class tuple.
    pub fn type_args(&self) -> Option<crate::items::TypeArgs> {
        self.0.children().find_map(crate::items::TypeArgs::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequireClause(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for RequireClause {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::REQUIRE_CLAUSE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl RequireClause {
    /// Raw token stream of the clause body (everything between `require`
    /// and the terminating `;`). Phase 4b re-parses this against a
    /// constraint mini-grammar.
    ///
    /// Whitespace is preserved so the downstream tokenizer can split
    /// `R in E96(...)` on the `in` keyword without false-merging into
    /// `RinE96(...)`. Comments inside the clause body are dropped.
    pub fn body_text(&self) -> String {
        let mut buf = String::new();
        let mut after_kw = false;
        for el in self.0.children_with_tokens() {
            if let Some(tok) = el.as_token() {
                if tok.kind() == SyntaxKind::REQUIRE_KW { after_kw = true; continue; }
                if tok.kind() == SyntaxKind::SEMI { break; }
                if after_kw && tok.kind() != SyntaxKind::COMMENT {
                    buf.push_str(tok.text());
                }
            } else if let Some(n) = el.as_node() {
                if after_kw {
                    buf.push_str(&n.text().to_string());
                }
            }
        }
        buf.trim().to_string()
    }
}
