use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use rowan::{SyntaxNode, TextRange, ast::SyntaxNodePtr};
use rowan::ast::AstNode;
use bhdl_parser::{SyntaxKind, BhdlLanguage};
use bhdl_ast::{
    SourceFile, HasName,
    items::{Board, Entity, ComponentDef, InterfaceDef, TypedefDef, ImportStmt, PartFamilyDef},
    enums::EnumDef,
    traits::{TraitDef, TraitImpl},
    common::{ParamDecl, PortDecl, NetDecl, ComponentInst, NetRef, PinDecl}, // Added PinDecl for v2.0
    hierarchical::EntityInst,
    v2_statements::ConnectionStmt,
    expr::{Expr, BinaryExpr},
    interfaces::{InterfaceSignal, InterfaceInst, SignalDirection},
    PowerDecl, GroundDecl,
};

use crate::symbol_table::{Symbol, SymbolKind, SymbolTable, PortDirectionKind}; // Use crate:: for local module
use crate::helpers::parse_expr_as_i64; // Use helper from local module
use crate::net_attributes::NetAttribute;
use crate::scope_registry::{ScopeRegistry, ScopeId, ScopeKind};
use bhdl_common::{GenericParam, GenericParamType, BhdlType};

// --- Pass 1: Build Global Scope & Definition Scopes Map ---

// Pass 1 Context: Uses ScopeRegistry arena — scopes live in the registry
// from the moment they are allocated, eliminating the old pattern of
// moving completed scopes out of a stack into a HashMap.
struct Pass1Context {
    registry: ScopeRegistry,
    /// Stack of active scope IDs (current scope is last).
    scope_stack: Vec<ScopeId>,
    // Track imported modules to avoid duplicate processing
    imported_modules: HashMap<String, ()>,
    // Base path for resolving relative imports
    base_path: PathBuf,
    // Alias specializations (alias Name = Generic<args>) collected during import processing
    alias_specializations: Vec<crate::passes::AliasSpecialization>,
    // Expansion recipes extracted from imported entity definitions
    expansion_recipes: HashMap<String, bhdl_common::ExpansionRecipe>,
    // Design recipes extracted from imported entity `design { }` blocks,
    // keyed by entity name → intent name → recipe.
    design_recipes: HashMap<String, HashMap<String, bhdl_common::design::DesignRecipe>>,
    // Symbol definitions extracted from imported files
    symbol_definitions: HashMap<String, bhdl_common::SymbolDefinition>,
    // Layout definitions extracted from imported files
    layout_definitions: HashMap<String, bhdl_common::LayoutDefinition>,
    // Placement recipes extracted from imported files
    placement_recipes: HashMap<String, bhdl_common::PlacementRecipe>,
    // Stage 6 cross-file: per-entity attribute-default index accumulated
    // from every imported file. Threaded into extract_expansion_recipes_
    // with_overlay so an expansion in file A that instantiates an entity
    // defined in file B picks up the callee's attribute defaults at
    // extraction time.
    entity_attribute_index: HashMap<String, HashMap<String, String>>,
    // Cross-file: per-entity ordered constructor-parameter names,
    // accumulated from every imported file. Threaded into
    // extract_expansion_recipes_with_overlay so the recipe extractor
    // can resolve attribute values that are bare references to a child
    // entity's own parameters (e.g. `attribute capacitance = value;`)
    // into the positional argument supplied at instantiation.
    entity_param_index: HashMap<String, Vec<String>>,
}

impl Pass1Context {
    fn new() -> Self {
        let registry = ScopeRegistry::new();
        let global_id = registry.global_id();
        Self {
            registry,
            scope_stack: vec![global_id],
            imported_modules: HashMap::new(),
            base_path: PathBuf::from("."),
            alias_specializations: Vec::new(),
            expansion_recipes: HashMap::new(),
            design_recipes: HashMap::new(),
            symbol_definitions: HashMap::new(),
            layout_definitions: HashMap::new(),
            placement_recipes: HashMap::new(),
            entity_attribute_index: HashMap::new(),
            entity_param_index: HashMap::new(),
        }
    }

    fn global_scope_mut(&mut self) -> &mut SymbolTable {
        self.registry.global_scope_mut()
    }

    fn current_scope_id(&self) -> ScopeId {
        *self.scope_stack.last().expect("Scope stack empty during Pass 1")
    }

    fn current_scope_mut(&mut self) -> &mut SymbolTable {
        let id = self.current_scope_id();
        self.registry.table_mut(id)
    }

    /// Allocate a new child scope under the current scope, register it
    /// for the given definition AST node, and push it onto the stack.
    fn push_scope(&mut self, def_node_ptr: SyntaxNodePtr<BhdlLanguage>, kind: ScopeKind) {
        let parent = self.current_scope_id();
        let child_id = self.registry.alloc_child(parent, kind);
        self.registry.register_node(def_node_ptr, child_id);
        self.scope_stack.push(child_id);
    }

    /// Pop the current scope off the stack. The scope remains in the
    /// registry (arena-allocated) — nothing is moved or dropped.
    fn pop_scope(&mut self) {
        if self.scope_stack.len() > 1 {
            self.scope_stack.pop();
        }
    }
}

// Populates global scope AND builds the map of definition_node -> its scope
pub fn populate_global_scope_and_build_definition_scopes(source_file: &SourceFile) -> (SymbolTable, HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>) {
    populate_global_scope_and_build_definition_scopes_with_base(source_file, Path::new("."))
}

// Populates global scope AND builds the map of definition_node -> its scope with specified base path
pub fn populate_global_scope_and_build_definition_scopes_with_base(
    source_file: &SourceFile,
    base_path: &Path
) -> (SymbolTable, HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>) {
    let (registry, _alias_specializations, _expansion_recipes, _symbol_defs, _layout_defs, _placement_recipes, _design_recipes, _entity_attr_index, _entity_param_index) = build_scope_registry_with_base(source_file, base_path);
    // Extract legacy data structures for backward compatibility
    let global_scope = registry.extract_global_scope();
    let definition_scopes = registry.extract_definition_scopes();
    (global_scope, definition_scopes)
}

/// Build a `ScopeRegistry` from a source file. This is the primary
/// entry point for Pass 1 — the tuple-returning functions above are
/// backward-compatible wrappers.
pub fn build_scope_registry(source_file: &SourceFile) -> ScopeRegistry {
    build_scope_registry_with_base(source_file, Path::new(".")).0
}

/// Build a `ScopeRegistry` with a base path for import resolution.
/// Returns the scope registry, alias specializations, and expansion recipes from imported entities.
pub fn build_scope_registry_with_base(
    source_file: &SourceFile,
    base_path: &Path,
) -> (
    ScopeRegistry,
    Vec<crate::passes::AliasSpecialization>,
    HashMap<String, bhdl_common::ExpansionRecipe>,
    HashMap<String, bhdl_common::SymbolDefinition>,
    HashMap<String, bhdl_common::LayoutDefinition>,
    HashMap<String, bhdl_common::PlacementRecipe>,
    HashMap<String, HashMap<String, bhdl_common::design::DesignRecipe>>,
    // Stage 6 cross-file: per-entity attribute defaults from imported
    // files. Threaded into the main-file's extract_expansion_recipes
    // call as an overlay so cross-file device discovery works for
    // boards too (a board imports a stage entity from a stdlib file,
    // the stage's expansion instantiates a device entity from a
    // different stdlib file, the device's attributes flow through).
    HashMap<String, HashMap<String, String>>,
    // Cross-file: per-entity ordered constructor-parameter names (see
    // Pass1Context::entity_param_index). Threaded into main-file recipe
    // extraction so attribute values referencing a child entity's own
    // parameters resolve to the instantiation's positional arguments.
    HashMap<String, Vec<String>>,
) {
    println!("Building scope registry (Pass 1)...");
    let mut context = Pass1Context::new();
    context.base_path = base_path.to_path_buf();

    // Seed built-in types into the global scope
    let dummy_range = TextRange::new(0.into(), 0.into());
    for type_name in &["signal", "power", "frequency", "voltage", "resistance", "percentage", "int"] {
        context.global_scope_mut().insert(Symbol {
            name: type_name.to_string(),
            kind: SymbolKind::Typedef,
            span: dummy_range,
            instance_type_name: None,
            definition_node_ptr: None,
            bus_high: None,
            bus_low: None,
            direction: None,
            parameter_overrides: None,
            net_attributes: None,
            resolved_type: Some(bhdl_common::BhdlType::from_type_name(type_name, None)),
            generic_params: None,
            when_condition: None,
            bus_size_param: None,
        });
    }

    // First pass: process imports
    for item in source_file.items() {
        if let Some(import) = ImportStmt::cast(item.syntax().clone()) {
            process_import(&import, &mut context);
        }
    }

    // Second pass: process the rest of the file
    visit_node_pass1_recursive(&source_file.syntax(), &mut context);

    println!("Completed Pass 1. Total symbols: {}, Scopes: {}",
             context.global_scope_mut().get_symbols().len(),
             context.registry.len());
    let alias_specializations = context.alias_specializations;
    let expansion_recipes = context.expansion_recipes;
    let symbol_definitions = context.symbol_definitions;
    let layout_definitions = context.layout_definitions;
    let placement_recipes = context.placement_recipes;
    let design_recipes = context.design_recipes;
    let entity_attribute_index = context.entity_attribute_index;
    let entity_param_index = context.entity_param_index;
    (context.registry, alias_specializations, expansion_recipes, symbol_definitions, layout_definitions, placement_recipes, design_recipes, entity_attribute_index, entity_param_index)
}

// Pass 1 recursive helper (takes Pass1Context)
fn visit_node_pass1_recursive(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass1Context) {
     let mut scope_pushed_for_this_node = false;

     match node.kind() {
        SyntaxKind::IMPORT_STMT => {
            // Skip imports - already processed in first pass
        }
        SyntaxKind::BOARD_DEF => {
            if let Some(def_node) = Board::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(),
                        SymbolKind::Board,
                        name_token.text_range(),
                        &node_ptr
                    ));
                    context.push_scope(node_ptr, ScopeKind::Board);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
        SyntaxKind::ENTITY_DEF => {
             if let Some(def_node) = Entity::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    let mut sym = Symbol::new_definition(
                        name_token.text(),
                        SymbolKind::Entity,
                        name_token.text_range(),
                        &node_ptr
                    );
                    sym.generic_params = extract_generic_params(&def_node);
                    context.current_scope_mut().insert(sym);
                    context.push_scope(node_ptr, ScopeKind::Entity);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
        SyntaxKind::COMPONENT_DEF => {
             if let Some(def_node) = ComponentDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(),
                        SymbolKind::Component,
                        name_token.text_range(),
                        &node_ptr
                    ));
                    context.push_scope(node_ptr, ScopeKind::Component);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
        SyntaxKind::INTERFACE_DEF => {
             if let Some(def_node) = InterfaceDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(),
                        SymbolKind::Interface,
                        name_token.text_range(),
                        &node_ptr
                    ));
                    context.push_scope(node_ptr, ScopeKind::Interface);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
         SyntaxKind::TYPEDEF_DEF => {
             if let Some(def_node) = TypedefDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(),
                        SymbolKind::Typedef,
                        name_token.text_range(),
                        &node_ptr
                    ));
                }
            }
        }
        SyntaxKind::PART_FAMILY_DEF => {
            // v0.2 catalog declaration. The symbol is keyed on the
            // family name (Yageo_RC0603FR_07, TI_LM317T, etc.); the
            // class pattern's entity name (Resistor, LM317) is stored
            // in `instance_type_name` so a later index can group all
            // families that populate the same entity for fast catalog
            // scan lookup. Phase 4c will build that index over the
            // top-level scope; Phase 4a only does the registration.
            if let Some(def_node) = PartFamilyDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let entity_name = def_node
                        .class_pattern()
                        .and_then(|cp| cp.entity_name());
                    let node_ptr = SyntaxNodePtr::new(node);
                    let mut sym = Symbol::new_definition(
                        name_token.text(),
                        SymbolKind::PartFamily,
                        name_token.text_range(),
                        &node_ptr,
                    );
                    sym.instance_type_name = entity_name;
                    context.current_scope_mut().insert(sym);
                }
            }
        }
        SyntaxKind::ENUM_DEF => {
            if let Some(def_node) = EnumDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let enum_name = name_token.text().to_string();
                    let node_ptr = SyntaxNodePtr::new(node);

                    // Register the enum type in the current scope
                    let mut enum_sym = Symbol::new_definition(
                        &enum_name,
                        SymbolKind::Enum,
                        name_token.text_range(),
                        &node_ptr,
                    );
                    enum_sym.resolved_type = Some(bhdl_common::BhdlType::Enum(enum_name.clone()));
                    context.current_scope_mut().insert(enum_sym);

                    // Register each variant as a symbol in the current scope
                    // Variants are accessible as EnumName::VariantName
                    for variant in def_node.variants() {
                        if let Some(variant_name_token) = variant.name() {
                            let variant_ptr = SyntaxNodePtr::new(variant.syntax());
                            let qualified_name = format!("{}::{}", enum_name, variant_name_token.text());
                            context.current_scope_mut().insert(Symbol::new_definition(
                                &qualified_name,
                                SymbolKind::EnumVariant,
                                variant_name_token.text_range(),
                                &variant_ptr,
                            ));
                        }
                    }
                }
            }
        }
        SyntaxKind::TRAIT_DEF => {
            if let Some(def_node) = TraitDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let trait_name = name_token.text().to_string();
                    let node_ptr = SyntaxNodePtr::new(node);

                    // Register the trait as a type in the current scope
                    let mut trait_sym = Symbol::new_definition(
                        &trait_name,
                        SymbolKind::Trait,
                        name_token.text_range(),
                        &node_ptr,
                    );
                    trait_sym.resolved_type = Some(bhdl_common::BhdlType::Trait(trait_name.clone()));
                    context.current_scope_mut().insert(trait_sym);
                }
            }
        }
        SyntaxKind::TRAIT_IMPL => {
            // Trait implementations are validated in Pass 2
            // Here we just note them so the scope registry knows about them
        }
        SyntaxKind::ALIAS => {
            // Handle same-file alias declarations: alias LM7805 = LinearRegulator<5V>;
            let mut alias_name = String::new();
            let mut target_name = String::new();
            let mut found_eq = false;
            let mut type_args = Vec::new();

            for element in node.children_with_tokens() {
                match element {
                    rowan::NodeOrToken::Token(t) => {
                        match t.kind() {
                            SyntaxKind::IDENT | SyntaxKind::NUMBER => {
                                if !found_eq && alias_name.is_empty() {
                                    alias_name = t.text().to_string();
                                } else if found_eq && target_name.is_empty() {
                                    target_name = t.text().to_string();
                                }
                            },
                            SyntaxKind::EQ => {
                                found_eq = true;
                            },
                            _ => {}
                        }
                    },
                    rowan::NodeOrToken::Node(n) => {
                        if n.kind() == SyntaxKind::TYPE_ARGS {
                            // Extract type argument texts from TYPE_ARGS node
                            for arg_element in n.children_with_tokens() {
                                if let rowan::NodeOrToken::Node(arg_node) = &arg_element {
                                    let text = arg_node.text().to_string().trim().to_string();
                                    if !text.is_empty() {
                                        type_args.push(text);
                                    }
                                } else if let rowan::NodeOrToken::Token(arg_token) = &arg_element {
                                    match arg_token.kind() {
                                        SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW => {
                                            type_args.push(arg_token.text().to_string());
                                        }
                                        SyntaxKind::NUMBER | SyntaxKind::IDENT => {
                                            // Standalone number or ident that isn't inside a VALUE node
                                            // (shouldn't normally happen since parse_value wraps these)
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !alias_name.is_empty() && !target_name.is_empty() {
                // Register the alias as an entity-like symbol in the global scope
                let node_ptr = SyntaxNodePtr::new(node);
                let sym = Symbol::new_definition(
                    &alias_name,
                    SymbolKind::Entity,
                    TextRange::new(0.into(), 0.into()),
                    &node_ptr,
                );
                context.current_scope_mut().insert(sym);

                // If this alias has type args, store it as an alias specialization
                if !type_args.is_empty() {
                    context.alias_specializations.push(
                        crate::passes::AliasSpecialization {
                            alias_name: alias_name.clone(),
                            target_entity: target_name.clone(),
                            type_arg_texts: type_args,
                            concrete_params: std::collections::BTreeMap::new(),
                        }
                    );
                }
            }
        }
        SyntaxKind::PARAM_DECL | SyntaxKind::PARAM_ASSIGN => {
            if let Some(decl) = ParamDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Parameter,
                        name_token.text_range(),
                        node,
                        None, 
                        None,
                        None, 
                    ));
                }
            }
        }
         SyntaxKind::PORT_DECL => { 
             if let Some(decl) = PortDecl::cast(node.clone()) {
               if let Some(name_token) = decl.name() {
                   let (bus_high, bus_low) = decl.bus_suffix()
                       .and_then(|suffix| suffix.range())
                       .map(|range_expr| (
                            range_expr.lhs().and_then(|v| parse_expr_as_i64(&v)),
                            range_expr.rhs().and_then(|v| parse_expr_as_i64(&v))
                       ))
                       .unwrap_or((None, None));
                   // Note: PinDecl doesn't have direction() method, 
                   // direction would be inferred from context or parent
                   let direction = None; // Placeholder - could be enhanced to look at context
                   
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin, 
                       name_token.text_range(), 
                       node,
                       bus_high, 
                       bus_low,
                       direction, 
                   ));
               }
           }
        }
        SyntaxKind::PIN_DECL => {
            if let Some(decl) = PinDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    let mut bus_high = None;
                    let mut bus_low = None;
                    let mut bus_size_param = None;

                    if let Some(suffix) = decl.bus_suffix() {
                        if let Some(range_expr) = suffix.range() {
                            // Range-based bus: [high:low]
                            bus_high = range_expr.lhs().and_then(|v| parse_expr_as_i64(&v));
                            bus_low = range_expr.rhs().and_then(|v| parse_expr_as_i64(&v));
                        } else if let Some(index_expr) = suffix.index_expr() {
                            // Single expression bus: [N] or [4]
                            if let Some(n) = parse_expr_as_i64(&index_expr) {
                                // Literal integer: pin X[4] means bus_high = n-1, bus_low = 0
                                bus_high = Some(n - 1);
                                bus_low = Some(0);
                            } else {
                                // Non-literal (e.g., IDENT like CHANNELS) → parameterized bus size
                                let text = index_expr.syntax().text().to_string().trim().to_string();
                                if !text.is_empty() {
                                    bus_size_param = Some(text);
                                }
                            }
                        }
                    }

                    // Get pin direction from AST
                    let direction = decl.direction()
                        .map(|token| match token.kind() {
                            SyntaxKind::IN_KW => PortDirectionKind::In,
                            SyntaxKind::OUT_KW => PortDirectionKind::Out,
                            SyntaxKind::INOUT_KW => PortDirectionKind::InOut,
                            _ => PortDirectionKind::In, // Default
                        });

                    // Check if this is a virtual pin
                    let symbol_kind = if decl.is_virtual() {
                        SymbolKind::VirtualPin
                    } else {
                        SymbolKind::Pin
                    };

                    // Extract when condition for conditional pins
                    let when_condition = decl.when_condition_text();

                    let mut sym = Symbol::new_decl(
                        name_token.text(),
                        symbol_kind,
                        name_token.text_range(),
                        node,
                        bus_high,
                        bus_low,
                        direction,
                    );
                    sym.when_condition = when_condition;
                    sym.bus_size_param = bus_size_param;

                    context.current_scope_mut().insert(sym);
                }
            }
        }
        SyntaxKind::NET_DECL => { 
            if let Some(decl) = NetDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    let (bus_high, bus_low) = decl.bus_suffix()
                        .and_then(|suffix| suffix.range())
                       .map(|range_expr| (
                           range_expr.lhs().and_then(|v| parse_expr_as_i64(&v)),
                           range_expr.rhs().and_then(|v| parse_expr_as_i64(&v))
                       ))
                        .unwrap_or((None, None));

                     context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Net,
                        name_token.text_range(), 
                        node,
                        bus_high, 
                        bus_low,
                        None, 
                    ));
                }
            }
        }
        SyntaxKind::INTERFACE_FIELD_DECL => {
            // `interface [~]Name[:perspective] field;` inside an entity body.
            // Materialise the interface's signals as Pin symbols on
            // the parent entity, named `field.signal`.
            //
            // v0.7: signal lookup follows the perspective selector
            // (`:slave`) if present; falls back to the first-declared
            // perspective if the interface has any, or the top-level
            // flat signal set (v0.6 single-implicit-perspective form).
            // Legacy `~Name` resolves to the second-declared
            // perspective when present, else flips the top-level
            // signals' directions (v0.6 semantics).
            let (type_name_opt, perspective_name, reversed, field_info) =
                parse_field_decl_tokens(node);

            if let (Some(type_name), Some((field, field_range))) =
                (type_name_opt.as_deref(), field_info)
            {
                // Resolve the interface definition through the global scope.
                let iface_def_node = context
                    .global_scope_mut()
                    .lookup(type_name)
                    .filter(|sym| sym.kind == SymbolKind::Interface)
                    .and_then(|sym| sym.definition_node_ptr.clone())
                    .and_then(|ptr| {
                        node.ancestors()
                            .last()
                            .map(|root| ptr.to_node(&root))
                    });

                if let Some(iface_node) = iface_def_node {
                    // Resolve the perspective → list of signal nodes.
                    let (signal_nodes, flip_directions) = resolve_perspective_signals(
                        &iface_node,
                        perspective_name.as_deref(),
                        reversed,
                    );

                    for child in signal_nodes {
                        let Some(signal) = InterfaceSignal::cast(child.clone()) else { continue; };
                        let Some(signal_name_tok) = signal.name() else { continue; };
                        let signal_name = signal_name_tok.text().to_string();

                        let mut direction = signal.direction().map(|d| match d {
                            SignalDirection::In => PortDirectionKind::In,
                            SignalDirection::Out => PortDirectionKind::Out,
                            SignalDirection::InOut => PortDirectionKind::InOut,
                        });
                        if flip_directions {
                            direction = direction.map(|d| match d {
                                PortDirectionKind::In => PortDirectionKind::Out,
                                PortDirectionKind::Out => PortDirectionKind::In,
                                PortDirectionKind::InOut => PortDirectionKind::InOut,
                            });
                        }

                        // Pin symbol name combines field + signal so it
                        // can be resolved through the dotted form
                        // `field.signal` at connection sites.
                        let pin_name = format!("{}.{}", field, signal_name);
                        let mut sym = Symbol::new_decl(
                            &pin_name,
                            SymbolKind::Pin,
                            field_range,
                            node,
                            None, // no bus bounds
                            None,
                            direction,
                        );
                        // Cross-reference: instance_type_name carries
                        // the interface type so later passes can find
                        // sibling signals of the same field.
                        sym.instance_type_name = Some(type_name.to_string());
                        context.current_scope_mut().insert(sym);
                    }
                }
            }
        }
        SyntaxKind::INTERFACE_SIGNAL => {
            if let Some(signal) = InterfaceSignal::cast(node.clone()) {
                if let Some(name_token) = signal.name() {
                    let direction = signal.direction().map(|d| match d {
                        SignalDirection::In => PortDirectionKind::In,
                        SignalDirection::Out => PortDirectionKind::Out,
                        SignalDirection::InOut => PortDirectionKind::InOut,
                    });
                    
                    context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(),
                        SymbolKind::Pin, // Interface signals are like pins
                        name_token.text_range(),
                        node,
                        None, // No bus bounds for now
                        None,
                        direction,
                    ));
                }
            }
        }
        SyntaxKind::INTERFACE_REQUIREMENT => {
            // Interface requirements don't create symbols, they are just constraints
            // Could be handled in a separate pass for validation
        }
        SyntaxKind::INTERFACE_INST => {
            if let Some(inst) = InterfaceInst::cast(node.clone()) {
                if let Some(name_token) = inst.name() {
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(),
                        SymbolKind::Instance,
                        name_token.text_range(),
                        &SyntaxNodePtr::new(node),
                    ));
                }
            }
        }
        SyntaxKind::COMPONENT_INST => {
             if let Some(inst) = ComponentInst::cast(node.clone()) {
                if let (Some(instance_name_token), Some(type_name_token)) = (inst.name(), inst.component_type_name()) {
                    let instance_name = instance_name_token.text().to_string();
                    let type_name = type_name_token.text().to_string();
                    
                    // Check if this is actually an interface instance by looking up the type
                    let is_interface = context.global_scope_mut()
                        .lookup(&type_name)
                        .map(|sym| sym.kind == SymbolKind::Interface)
                        .unwrap_or(false);
                    
                    let mut instance_symbol = Symbol::new_instance(
                        &instance_name, 
                        instance_name_token.text_range(),
                        &type_name, 
                        node,
                    );
                    let mut overrides_map = HashMap::new();
                        if let Some(param_block) = inst.param_assign_block() {
                            for param_assign in param_block.assignments() {
                                        let param_name_token = param_assign.name();
                                        let value_expr_node = param_assign.syntax().children_with_tokens()
                                            .skip_while(|e| e.kind() != SyntaxKind::EQ) 
                                            .skip(1) 
                                            .filter_map(|e| e.into_node()) 
                                            .find(|n| !matches!(n.kind(), SyntaxKind::WHITESPACE | SyntaxKind::SEMI)); 
                                        if let (Some(param_name), Some(value_expr)) = (param_name_token, value_expr_node) {
                                            overrides_map.insert(
                                                param_name.text().to_string(),
                                                SyntaxNodePtr::new(&value_expr)
                                            );
                                        }
                        }
                    }
                    if !overrides_map.is_empty() {
                        instance_symbol.parameter_overrides = Some(overrides_map);
                    }
                    context.current_scope_mut().insert(instance_symbol);
                    return; 
                } 
            }
        }
        SyntaxKind::ENTITY_INST => {
            if let Some(inst) = EntityInst::cast(node.clone()) {
                if let (Some(instance_name_token), Some(type_name_token)) = (inst.name(), inst.entity_type()) {
                    let instance_name = instance_name_token.text().to_string();
                    let type_name = type_name_token.text().to_string();
                    let mut instance_symbol = Symbol::new_instance(
                        &instance_name, 
                        instance_name_token.text_range(),
                        &type_name, 
                        node,
                    );
                    
                    
                    // Handle parameter overrides
                    let mut overrides_map = HashMap::new();
                    if let Some(param_list) = inst.param_list() {
                        // Process entity parameters (both positional and named)
                        for param_assign in param_list.params() {
                            if let Some(param_name_token) = param_assign.name() {
                                let value_expr_node = param_assign.syntax().children_with_tokens()
                                    .skip_while(|e| e.kind() != SyntaxKind::EQ) 
                                    .skip(1) 
                                    .filter_map(|e| e.into_node()) 
                                    .find(|n| !matches!(n.kind(), SyntaxKind::WHITESPACE | SyntaxKind::SEMI)); 
                                if let Some(value_expr) = value_expr_node {
                                    overrides_map.insert(
                                        param_name_token.text().to_string(),
                                        SyntaxNodePtr::new(&value_expr)
                                    );
                                }
                            }
                        }
                    }
                    
                    if !overrides_map.is_empty() {
                        instance_symbol.parameter_overrides = Some(overrides_map);
                    }
                    
                    context.current_scope_mut().insert(instance_symbol);
                    
                    // Push a new scope for the entity instance body (port mappings)
                    context.push_scope(SyntaxNodePtr::new(node), ScopeKind::EntityInstance);
                    context.current_scope_mut().set_scope_name(format!("{}::{}", instance_name, type_name));
                    scope_pushed_for_this_node = true;
                } 
            }
        }
        SyntaxKind::CONNECTION_STMT => {
            // Process connections to create net symbols from @ syntax
            visit_connection_for_nets(node, context);
        }
        SyntaxKind::POWER_DECL => {
            // Create net symbol for power declaration with attributes
            if let Some(power_decl) = PowerDecl::cast(node.clone()) {
                if let Some(name_token) = power_decl.name() {
                    let name = name_token.text();
                    
                    // Parse voltage and current values
                    let voltage = power_decl.voltage()
                        .and_then(|v| parse_electrical_value_str(&v))
                        .unwrap_or(0.0);
                    
                    let current = power_decl.current()
                        .and_then(|c| parse_electrical_value_str(&c))
                        .unwrap_or(1.0);
                    
                    // Power domains are nets with special attributes
                    let mut power_symbol = Symbol::new_decl(
                        name,
                        SymbolKind::Net,
                        name_token.text_range(),
                        node,
                        None, // No bus bounds
                        None,
                        None, // No direction for nets
                    );
                    
                    // Add power domain attributes
                    let mut attr = NetAttribute::new_power_domain(voltage, current);

                    // Extract stage chain: |> stage1(params) |> stage2
                    let stages = power_decl.stages_with_params();
                    if !stages.is_empty() {
                        attr.set_stages(stages);
                    }

                    power_symbol.net_attributes = Some(attr);

                    context.current_scope_mut().insert(power_symbol);
                }
            }
        }
        SyntaxKind::GROUND_DECL => {
            // Create net symbol for ground declaration with attributes
            if let Some(ground_decl) = GroundDecl::cast(node.clone()) {
                if let Some(name_token) = ground_decl.name() {
                    let name = name_token.text();
                    // Ground is a net with 0V
                    let mut ground_symbol = Symbol::new_decl(
                        name,
                        SymbolKind::Net,
                        name_token.text_range(),
                        node,
                        None, // No bus bounds
                        None,
                        None, // No direction for nets
                    );
                    
                    // Add ground domain attributes
                    ground_symbol.net_attributes = Some(NetAttribute::new_ground_domain());
                    
                    context.current_scope_mut().insert(ground_symbol);
                }
            }
        }
        _ => {} 
     }
     
     for child in node.children() {
         visit_node_pass1_recursive(&child, context);
     }
     
     if scope_pushed_for_this_node { 
         context.pop_scope(); 
     }
}

// Helper function to parse electrical value from string
fn parse_electrical_value_str(value_str: &str) -> Option<f64> {
    // Handle common electrical units with multipliers
    let (num_str, unit) = if let Some(pos) = value_str.find(|c: char| c.is_alphabetic() || c == 'Ω') {
        (&value_str[..pos], &value_str[pos..])
    } else {
        (value_str, "")
    };
    
    let base_value = num_str.parse::<f64>().ok()?;
    
    // Apply unit multipliers
    let multiplier = match unit {
        // Current units
        "mA" => 0.001,
        "uA" | "μA" => 0.000001,
        "A" => 1.0,
        // Voltage units  
        "mV" => 0.001,
        "uV" | "μV" => 0.000001,
        "V" => 1.0,
        "kV" => 1000.0,
        // Resistance units
        "mΩ" | "mohm" => 0.001,
        "Ω" | "ohm" => 1.0,
        "kΩ" | "kohm" => 1000.0,
        "MΩ" | "Mohm" => 1_000_000.0,
        // Capacitance units
        "pF" => 1e-12,
        "nF" => 1e-9,
        "uF" | "μF" => 1e-6,
        "mF" => 1e-3,
        "F" => 1.0,
        // Default: no unit
        _ => 1.0,
    };
    
    Some(base_value * multiplier)
}

// Helper function to process connections and create net symbols from @ syntax
fn visit_connection_for_nets(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass1Context) {
    if let Some(conn_stmt) = ConnectionStmt::cast(node.clone()) {
        if let Some(expr_node) = conn_stmt.expr() {
            // Collect all net references in the connection
            let net_refs = collect_net_refs_in_flow(&expr_node);
            // Check if the CONNECTION_STMT itself has multiple binary expr children
            let binary_exprs: Vec<_> = node.children()
                .filter(|n| n.kind() == SyntaxKind::BINARY_EXPR)
                .collect();
            
            if binary_exprs.len() >= 2 {
                // The pattern for implicit net creation: first binary expr ends with @net
                // and there's another binary expr following (meaning @net is in the middle)
                if let Some(first_binary) = BinaryExpr::cast(binary_exprs[0].clone()) {
                    if first_binary.op() == Some(SyntaxKind::ARROW) {
                        if let Some(rhs) = first_binary.rhs() {
                            if let Some(Expr::NetRef(net_ref)) = Expr::cast(rhs.syntax().clone()) {
                                if let Some(name) = net_ref.name() {
                                    if context.current_scope_mut().lookup_net(&name).is_none() {
                                        let net_symbol = Symbol::new_decl(
                                            &name,
                                            SymbolKind::Net,
                                            net_ref.syntax().text_range(),
                                            net_ref.syntax(),
                                            None,
                                            None,
                                            None,
                                        );
                                        context.current_scope_mut().insert(net_symbol);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Note: We don't create implicit nets for standalone @ references
            // Those will be checked in pass2 and produce errors if undefined
        }
    }
}

// Collect all net references in a flow expression
fn collect_net_refs_in_flow(node: &SyntaxNode<BhdlLanguage>) -> Vec<(String, TextRange)> {
    let mut net_refs = Vec::new();
    collect_net_refs_recursive(node, &mut net_refs);
    net_refs
}

// Recursively collect net references
fn collect_net_refs_recursive(node: &SyntaxNode<BhdlLanguage>, net_refs: &mut Vec<(String, TextRange)>) {
    if node.kind() == SyntaxKind::NET_REF {
        if let Some(net_ref) = NetRef::cast(node.clone()) {
            if let Some(name) = net_ref.name() {
                net_refs.push((name, node.text_range()));
            }
        }
    }
    
    // Recurse into children
    for child in node.children() {
        collect_net_refs_recursive(&child, net_refs);
    }
}

/// Extract generic parameters from an entity definition's AST.
/// Maps type bound names (e.g., "voltage", "resistance") to BhdlType variants.
fn extract_generic_params(entity: &Entity) -> Option<Vec<GenericParam>> {
    let generic_params_node = entity.generic_params()?;
    let params: Vec<GenericParam> = generic_params_node.params()
        .filter_map(|param_node| {
            let name = param_node.name()?.text().to_string();
            let param_type = if let Some(bound) = param_node.type_bound() {
                let bhdl_type = BhdlType::from_type_name(&bound, None);
                if bhdl_type == BhdlType::Unknown {
                    // Treat unknown bounds as trait bounds (e.g., "Passive")
                    GenericParamType::TypeBounded(vec![bound])
                } else {
                    GenericParamType::Const(bhdl_type)
                }
            } else {
                GenericParamType::Type
            };
            // Extract default value if present
            let default = param_node.default_value().and_then(|expr| {
                match &expr {
                    Expr::Value(val) => crate::helpers::parse_value_as_const(val),
                    _ => {
                        // For non-Value expressions, try to get the text
                        let text = expr.syntax().text().to_string();
                        match text.trim() {
                            "true" => Some(bhdl_common::ConstValue::Bool(true)),
                            "false" => Some(bhdl_common::ConstValue::Bool(false)),
                            other => other.parse::<i64>().ok().map(bhdl_common::ConstValue::Integer),
                        }
                    }
                }
            });

            Some(GenericParam {
                name,
                param_type,
                constraints: Vec::new(), // TODO: extract from where clause
                default,
            })
        })
        .collect();

    if params.is_empty() {
        None
    } else {
        Some(params)
    }
}

// Process an import statement
fn process_import(import: &ImportStmt, context: &mut Pass1Context) {
    // Get the import path
    let path = match import.path() {
        Some(p) => p,
        None => {
            println!("Warning: Import statement has no path");
            return;
        }
    };
    
    // Check if already imported
    if context.imported_modules.contains_key(&path) {
        return;
    }
    
    // Mark as imported
    context.imported_modules.insert(path.clone(), ());
    
    // Get the imported names (for destructuring imports)
    let imported_names = import.imported_names();
    let is_destructuring = !imported_names.is_empty();
    
    // Resolve the import path
    let resolved_path = resolve_import_path(&path, &context.base_path);
    
    // Load and parse the imported file
    match load_and_parse_module(&resolved_path) {
        Ok(imported_source) => {
            // Stage 6 cross-file: recursively process the imported
            // file's OWN imports first, so a chain like
            //   board → stage.bhdl → tube.bhdl
            // makes the tube's entity attributes reachable in
            // entity_attribute_index by the time we extract the stage's
            // recipes below. Cycle detection lives in the
            // `imported_modules` set (top of this function), which is
            // already keyed by path string.
            for item in imported_source.items() {
                if let Some(nested_import) = ImportStmt::cast(item.syntax().clone()) {
                    process_import(&nested_import, context);
                }
            }
            // Stage 6: merge this file's entity attribute defaults into
            // the global cross-file index BEFORE recipe extraction, so
            // an entity in this file referenced via expansion from a
            // sibling or downstream file gets its attributes attached.
            for (name, attrs) in crate::extract_entity_attribute_index(&imported_source) {
                context.entity_attribute_index.insert(name, attrs);
            }
            for (name, params) in crate::extract_entity_param_names(&imported_source) {
                context.entity_param_index.insert(name, params);
            }
            // Extract expansion recipes from imported entities, threading
            // the accumulated cross-file index in so children carry
            // attributes for entities defined in other imports.
            let imported_recipes = crate::extract_expansion_recipes_with_overlay(
                &imported_source,
                &context.entity_attribute_index,
            );
            for (name, recipe) in imported_recipes {
                context.expansion_recipes.insert(name, recipe);
            }
            // Extract vendor `design { }` recipes from imported entities
            let imported_designs = crate::extract_design_recipes(&imported_source);
            for (entity, by_intent) in imported_designs {
                context.design_recipes.entry(entity).or_default().extend(by_intent);
            }

            // Extract symbol and layout definitions from imported files
            let imported_symbols = crate::extract_symbol_definitions(&imported_source);
            for (name, sym) in imported_symbols {
                context.symbol_definitions.insert(name, sym);
            }
            let imported_layouts = crate::extract_layout_definitions(&imported_source);
            for (name, lay) in imported_layouts {
                context.layout_definitions.insert(name, lay);
            }
            let imported_placements = crate::extract_placement_recipes(&imported_source);
            for (name, pr) in imported_placements {
                context.placement_recipes.insert(name, pr);
            }

            // First, build a map of all entities in the file
            let mut available_entities = std::collections::HashMap::new();
            for item in imported_source.items() {
                if let Some(entity) = Entity::cast(item.syntax().clone()) {
                    if let Some(name_token) = entity.name() {
                        let entity_name = name_token.text().to_string();
                        available_entities.insert(entity_name, entity);
                    }
                }
            }
            
            // Then, process aliases to find what maps to requested names
            // alias_map: alias_name → target_name
            // alias_type_args: alias_name → Vec<type_arg_text>
            let mut aliases = std::collections::HashMap::new();
            let mut alias_type_args: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
            for child in imported_source.syntax().children() {
                if child.kind() == SyntaxKind::ALIAS {
                    // Parse alias: extract name, target, and optional type args
                    let mut alias_name = String::new();
                    let mut target_name = String::new();
                    let mut found_eq = false;

                    for element in child.children_with_tokens() {
                        match element {
                            rowan::NodeOrToken::Token(t) => {
                                match t.kind() {
                                    SyntaxKind::IDENT => {
                                        if !found_eq && alias_name.is_empty() {
                                            alias_name = t.text().to_string();
                                        } else if found_eq && target_name.is_empty() {
                                            target_name = t.text().to_string();
                                        }
                                    },
                                    SyntaxKind::EQ => {
                                        found_eq = true;
                                    },
                                    _ => {}
                                }
                            }
                            rowan::NodeOrToken::Node(n) => {
                                if n.kind() == SyntaxKind::TYPE_ARGS {
                                    // Extract type arg expressions as text
                                    let mut args = Vec::new();
                                    for arg_child in n.children() {
                                        // Each child expression is a type arg
                                        let arg_text = arg_child.text().to_string().trim().to_string();
                                        if !arg_text.is_empty() {
                                            args.push(arg_text);
                                        }
                                    }
                                    if !args.is_empty() {
                                        alias_type_args.insert(alias_name.clone(), args);
                                    }
                                }
                            }
                        }
                    }

                    if !alias_name.is_empty() && !target_name.is_empty() {
                        aliases.insert(alias_name, target_name);
                    }
                }
            }
            
            // Register expansion recipes under alias names too.
            // e.g., if BuckRegulator has a recipe and alias LM2596_5V = BuckRegulator<5V>,
            // register the recipe under "LM2596_5V" as well.
            for (alias_name, target_name) in &aliases {
                if let Some(recipe) = context.expansion_recipes.get(target_name) {
                    // Copy the recipe under the alias name but KEEP its
                    // original `entity_name` (the target). The expansion
                    // interpreter looks up the matching `design { }` recipe
                    // by `recipe.entity_name`; renaming it to the alias
                    // would miss the design recipe (which is keyed by the
                    // real entity), silently dropping computed values for
                    // SKU-alias instances.
                    let alias_recipe = recipe.clone();
                    context.expansion_recipes.insert(alias_name.clone(), alias_recipe);
                }
            }

            // Now process the imports
            if is_destructuring {
                // Only import the requested entities and their aliases
                for requested_name in &imported_names {
                    // Check if it's an alias
                    if let Some(target_name) = aliases.get(requested_name) {
                        if let Some(entity) = available_entities.get(target_name) {
                            // Check if this alias has type args (e.g., alias LM7805 = LinearRegulator<5V>)
                            if let Some(type_args) = alias_type_args.get(requested_name) {
                                // Also import the generic entity under its own name
                                if context.global_scope_mut().lookup(target_name).is_none() {
                                    process_imported_entity(entity, target_name, context);
                                }
                                // Import under the alias name (non-generic copy)
                                process_imported_entity(entity, requested_name, context);
                                // Store alias specialization for monomorphization
                                context.alias_specializations.push(
                                    crate::passes::AliasSpecialization {
                                        alias_name: requested_name.clone(),
                                        target_entity: target_name.clone(),
                                        type_arg_texts: type_args.clone(),
                                        concrete_params: std::collections::BTreeMap::new(),
                                    }
                                );
                            } else {
                                // Simple alias without type args
                                process_imported_entity(entity, requested_name, context);
                            }
                        }
                    } else {
                        // Direct entity import
                        if let Some(entity) = available_entities.get(requested_name) {
                            process_imported_entity(entity, requested_name, context);
                        }
                    }
                }
            } else {
                // Import all entities (old behavior)
                for (entity_name, entity) in &available_entities {
                    process_imported_entity(entity, entity_name, context);
                }
                // Also process aliases with type args from the file
                for (alias_name, target_name) in &aliases {
                    if let Some(type_args) = alias_type_args.get(alias_name) {
                        if let Some(entity) = available_entities.get(target_name) {
                            // Import under alias name
                            process_imported_entity(entity, alias_name, context);
                            // Store alias specialization
                            context.alias_specializations.push(
                                crate::passes::AliasSpecialization {
                                    alias_name: alias_name.clone(),
                                    target_entity: target_name.clone(),
                                    type_arg_texts: type_args.clone(),
                                    concrete_params: std::collections::BTreeMap::new(),
                                }
                            );
                        }
                    } else if let Some(entity) = available_entities.get(target_name) {
                        // Simple alias
                        process_imported_entity(entity, alias_name, context);
                    }
                }
            }
            
            // Also process component definitions if not destructuring
            if !is_destructuring {
                for item in imported_source.items() {
                    if let Some(component) = ComponentDef::cast(item.syntax().clone()) {
                        if let Some(name_token) = component.name() {
                            let node_ptr = SyntaxNodePtr::new(component.syntax());
                                    
                            let mut symbol = Symbol::new_definition(
                                name_token.text(), 
                                SymbolKind::Component,
                                name_token.text_range(), 
                                &node_ptr
                            );
                            
                            symbol.definition_node_ptr = Some(node_ptr.clone());
                            context.global_scope_mut().insert(symbol);
                            
                            // Create and populate component scope in the registry
                            let comp_scope_id = context.registry.alloc_child(
                                context.registry.global_id(), ScopeKind::Component);
                            context.registry.register_node(node_ptr, comp_scope_id);
                            context.registry.table_mut(comp_scope_id)
                                .set_scope_name(name_token.text().to_string());
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("Error loading import '{}': {}", path, e);
        }
    }
}

// Resolve import path relative to base
fn resolve_import_path(import_path: &str, base_path: &Path) -> PathBuf {
    // Handle different import path formats
    if import_path.starts_with("bhdl-stdlib/") {
        // Standard library import
        PathBuf::from(import_path)
    } else if import_path.starts_with('/') {
        // Absolute path
        PathBuf::from(import_path)
    } else {
        // Relative path
        base_path.join(import_path)
    }
}

// Load and parse a module file
fn load_and_parse_module(path: &Path) -> Result<SourceFile, String> {
    // Read the file
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file {:?}: {}", path, e))?;
    
    // Parse it
    let parsed = bhdl_parser::parse(&content);
    
    if !parsed.errors().is_empty() {
        return Err(format!("Parse errors in {:?}: {:?}", path, parsed.errors()));
    }
    
    // Get the AST
    let syntax = parsed.syntax();
    SourceFile::cast(syntax)
        .ok_or_else(|| format!("Failed to cast to SourceFile for {:?}", path))
}

// Process an imported entity and add it to the symbol table
fn process_imported_entity(entity: &Entity, name: &str, context: &mut Pass1Context) {
    let node_ptr = SyntaxNodePtr::new(entity.syntax());

    // Create symbol with the specified name (could be an alias)
    let mut symbol = Symbol::new_definition(
        name,
        SymbolKind::Entity,
        entity.syntax().text_range(),
        &node_ptr
    );

    // Store the imported entity definition node
    symbol.definition_node_ptr = Some(node_ptr.clone());

    // Extract generic parameters if present
    symbol.generic_params = extract_generic_params(entity);

    // Add to global scope
    context.global_scope_mut().insert(symbol);

    // Create a scope for this entity definition in the registry
    let entity_scope_id = context.registry.alloc_child(
        context.registry.global_id(), ScopeKind::Entity);
    context.registry.register_node(node_ptr, entity_scope_id);
    context.registry.table_mut(entity_scope_id).set_scope_name(name.to_string());

    // Process entity body to populate the scope
    process_entity_body(entity, context.registry.table_mut(entity_scope_id));
}

// Process entity body to extract pins, parameters, etc.
fn process_entity_body(entity: &Entity, scope: &mut SymbolTable) {
    // Process pins
    for pin in entity.pins() {
        if let Some(name) = pin.name() {
            let mut pin_symbol = Symbol {
                name: name.text().to_string(),
                kind: SymbolKind::Pin,
                span: name.text_range(),
                instance_type_name: None,
                definition_node_ptr: Some(SyntaxNodePtr::new(pin.syntax())),
                bus_high: None,
                bus_low: None,
                direction: None, // Would need to parse pin direction
                parameter_overrides: None,
                net_attributes: None,
                resolved_type: None,
                generic_params: None,
                when_condition: None,
                bus_size_param: None,
            };
            // Extract when condition and bus size param for imported entities
            pin_symbol.when_condition = pin.when_condition_text();
            if let Some(suffix) = pin.bus_suffix() {
                if suffix.range().is_none() {
                    if let Some(index_expr) = suffix.index_expr() {
                        if crate::helpers::parse_expr_as_i64(&index_expr).is_none() {
                            let text = index_expr.syntax().text().to_string().trim().to_string();
                            if !text.is_empty() {
                                pin_symbol.bus_size_param = Some(text);
                            }
                        }
                    }
                }
            }
            scope.insert(pin_symbol);
        }
    }
    
    // Process parameters
    if let Some(param_list) = entity.param_list() {
        for param in param_list.param_defs() {
        if let Some(name) = param.name() {
            let param_symbol = Symbol {
                name: name.text().to_string(),
                kind: SymbolKind::Parameter,
                span: name.text_range(),
                instance_type_name: None,
                definition_node_ptr: Some(SyntaxNodePtr::new(param.syntax())),
                bus_high: None,
                bus_low: None,
                direction: None,
                parameter_overrides: None,
                net_attributes: None,
                resolved_type: None,
                generic_params: None,
                when_condition: None,
                bus_size_param: None,
            };
            scope.insert(param_symbol);
        }
    }
    }
}


 
// ─────────────────────────────────────────────────────────────────
// v0.7 interface-perspective helpers
// ─────────────────────────────────────────────────────────────────

/// Walk an INTERFACE_FIELD_DECL node's tokens and pull out the four
/// pieces of information the analyser needs:
///   - type IDENT (interface name)
///   - perspective IDENT after a `:` (None if no selector)
///   - whether `~` was present (legacy reversal sigil)
///   - field IDENT + its TextRange
///
/// Grammar layout: `'interface' '~'? IDENT (':' IDENT)? ('<' … '>')? IDENT (';' | '{' … '}')`.
/// We classify tokens by appearance order. The first IDENT is the
/// type name; if a COLON follows, the next IDENT is the perspective;
/// any subsequent IDENT is the field name. Generic-args (`<...>`)
/// and the binding block (`{...}`) are non-IDENT-bearing children
/// at the relevant points so this counter-based pass works.
fn parse_field_decl_tokens(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
) -> (Option<String>, Option<String>, bool, Option<(String, TextRange)>) {
    use bhdl_ast::SyntaxKind;

    let tokens: Vec<_> = node
        .children_with_tokens()
        .filter_map(|el| el.into_token())
        .filter(|t| matches!(
            t.kind(),
            SyntaxKind::TILDE | SyntaxKind::COLON | SyntaxKind::IDENT
        ))
        .collect();

    let mut reversed = false;
    let mut type_name: Option<String> = None;
    let mut perspective: Option<String> = None;
    let mut field: Option<(String, TextRange)> = None;

    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        match tok.kind() {
            SyntaxKind::TILDE => {
                reversed = true;
                i += 1;
            }
            SyntaxKind::IDENT if type_name.is_none() => {
                type_name = Some(tok.text().to_string());
                i += 1;
            }
            SyntaxKind::COLON if perspective.is_none() => {
                i += 1;
                if i < tokens.len() && tokens[i].kind() == SyntaxKind::IDENT {
                    perspective = Some(tokens[i].text().to_string());
                    i += 1;
                }
            }
            SyntaxKind::IDENT if field.is_none() => {
                field = Some((tok.text().to_string(), tok.text_range()));
                i += 1;
            }
            _ => i += 1,
        }
    }

    (type_name, perspective, reversed, field)
}

/// Choose the set of INTERFACE_SIGNAL nodes to materialise for a
/// field declaration, plus whether the directions need flipping.
///
/// Resolution order:
///   1. Explicit `perspective_name` selector → that perspective's signals.
///   2. Legacy `~` sigil:
///        * 2+ perspectives declared → second-declared (no flip).
///        * 1 or 0 perspectives → top-level signals, FLIP directions
///          (v0.6 semantics).
///   3. No selector, no `~`:
///        * Interface has perspectives → first-declared (no flip).
///        * Interface has only top-level signals → top-level (no flip).
///
/// Returns `(signals, flip_directions)`. `flip_directions=true`
/// applies only in the legacy-`~`-with-no-explicit-perspectives
/// fallback case.
fn resolve_perspective_signals(
    iface: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    perspective: Option<&str>,
    legacy_reversed: bool,
) -> (Vec<rowan::SyntaxNode<bhdl_parser::BhdlLanguage>>, bool) {
    use bhdl_ast::SyntaxKind;

    let perspectives: Vec<_> = iface
        .children()
        .filter(|n| n.kind() == SyntaxKind::INTERFACE_PERSPECTIVE)
        .collect();

    // 1. Explicit selector.
    if let Some(name) = perspective {
        for p in &perspectives {
            let p_name = p
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| t.kind() == SyntaxKind::IDENT)
                .map(|t| t.text().to_string());
            if p_name.as_deref() == Some(name) {
                let sigs = p
                    .children()
                    .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
                    .collect();
                return (sigs, false);
            }
        }
        // Perspective name didn't match — fall through to default.
    }

    // 2. Legacy `~` sigil.
    if legacy_reversed {
        if perspectives.len() >= 2 {
            let sigs = perspectives[1]
                .children()
                .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
                .collect();
            return (sigs, false);
        }
        // Fall through to top-level with directions flipped.
        let sigs = iface
            .children()
            .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
            .collect();
        return (sigs, true);
    }

    // 3. No selector. Prefer first-declared perspective if any.
    if !perspectives.is_empty() {
        let sigs = perspectives[0]
            .children()
            .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
            .collect();
        return (sigs, false);
    }

    // 3 fallback: top-level signals (v0.6 single-implicit-perspective form).
    let sigs = iface
        .children()
        .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
        .collect();
    (sigs, false)
}
