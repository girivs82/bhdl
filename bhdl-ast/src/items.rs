// BHDL v2.0 AST Items
// Only supports v2.0 flow-based syntax

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken, HasName};
use rowan::ast::AstNode;
use crate::blocks::{LayerStackupBlock, DefaultDesignRulesBlock, ConstrainBlock};
use crate::common::{TypeRef, ParamAssign, ComponentInst, PortDecl, PinDecl};
use crate::v2_statements::ConnectionStmt;
use crate::expr::Expr;
use crate::v2_statements::{PowerDecl, GroundDecl, FlowStmt};
use crate::hierarchical::ModuleInst;
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
    
    // Hierarchical module support
    pub fn module_instances(&self) -> impl Iterator<Item = ModuleInst> {
        self.0.children().filter_map(ModuleInst::cast)
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
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Module(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for Module {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::MODULE_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for Module {}

impl Module {
    // v2.0 modules have pins and metadata
    pub fn pins(&self) -> impl Iterator<Item = PinDecl> {
        self.0.children().filter_map(PinDecl::cast)
    }
    
    // Keep ports() for compatibility with higher-level modules that might use ports
    pub fn ports(&self) -> impl Iterator<Item = PortDecl> {
        self.0.children().filter_map(PortDecl::cast)
    }
    
    pub fn param_list(&self) -> Option<ParamList> {
        self.0.children().find_map(ParamList::cast)
    }
    
    // Hierarchical module support
    pub fn module_instances(&self) -> impl Iterator<Item = ModuleInst> {
        self.0.children().filter_map(ModuleInst::cast)
    }
    
    pub fn component_instances(&self) -> impl Iterator<Item = ComponentInst> {
        self.0.children().filter_map(ComponentInst::cast)
    }
    
    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        self.0.children().filter_map(ConnectionStmt::cast)
    }
    
    // Module metadata (attributes)
    pub fn attributes(&self) -> impl Iterator<Item = AttributeDecl> {
        self.0.children().filter_map(AttributeDecl::cast)
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
    
    /// Get module parameter definitions (name: type = default)
    pub fn param_defs(&self) -> impl Iterator<Item = ModuleParam> {
        self.0.children().filter_map(ModuleParam::cast)
    }
    
    /// Check if this is a module parameter list (has type annotations)
    pub fn is_module_params(&self) -> bool {
        // Module params have COLON tokens for type annotations
        self.0.children_with_tokens()
            .any(|element| element.as_token()
                .map(|token| token.kind() == SyntaxKind::COLON)
                .unwrap_or(false))
    }
}

/// Module parameter definition: name: type = default_value
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleParam(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ModuleParam {
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

impl ModuleParam {
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