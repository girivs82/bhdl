//! Attribute extraction from BHDL AST nodes

use std::collections::HashMap;
use rowan::ast::AstNode;
use bhdl_ast::{Entity, Board};
use bhdl_parser::SyntaxKind;

/// Extract attributes from an entity's syntax tree
pub fn extract_module_attributes(entity: &Entity) -> HashMap<String, String> {
    let mut attributes = HashMap::new();

    let syntax = entity.syntax();

    for child in syntax.children() {
        // ATTRIBUTE_DECL nodes contain: ATTRIBUTE_KW IDENT EQ <expr> SEMI
        if child.kind() == SyntaxKind::ATTRIBUTE_DECL {
            let mut name: Option<String> = None;
            let mut found_eq = false;

            for elem in child.children_with_tokens() {
                match elem {
                    rowan::NodeOrToken::Token(token) => {
                        match token.kind() {
                            SyntaxKind::ATTRIBUTE_KW => { /* skip keyword */ }
                            SyntaxKind::IDENT if name.is_none() => {
                                name = Some(token.text().to_string());
                            }
                            SyntaxKind::EQ => {
                                found_eq = true;
                            }
                            SyntaxKind::SEMI => { /* skip semicolon */ }
                            SyntaxKind::WHITESPACE => { /* skip whitespace */ }
                            _ if found_eq && name.is_some() => {
                                // Simple token value (number, true/false, etc.)
                                let value = token.text().to_string();
                                if let Some(attr_name) = name.take() {
                                    attributes.insert(attr_name, value.trim().to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                    rowan::NodeOrToken::Node(node) if found_eq && name.is_some() => {
                        // Expression node — extract its text content
                        let value = extract_node_value(&node);
                        if let Some(attr_name) = name.take() {
                            attributes.insert(attr_name, value);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    attributes
}

/// Like [`extract_module_attributes`], but additionally resolves attribute
/// values that are bare references to the entity's OWN `const`s or
/// constructor parameters into their concrete value text:
///
/// ```text
///   attribute f_sw = f_sw;                       -> "570kHz" (param default)
///   attribute switching_frequency =
///       BUCK_PARAMS.switching_frequency;         -> "500kHz" (const field)
///   attribute oi = BUCK_PARAMS.impedance.output_impedance;  -> "0.05Ω"
/// ```
///
/// `extract_module_attributes` returns the literal *reference text*
/// (`"f_sw"`, `"BUCK_PARAMS.switching_frequency"`) because it only reads the
/// value token. The sign-off ripple model needs the resolved NUMBER, so this
/// variant evaluates the reference against the entity's local declarations.
/// Anything that doesn't resolve (a call-site-only param with no default, an
/// unknown reference, an expression) is left exactly as
/// `extract_module_attributes` produced it — the resolution is purely
/// additive and never fails.
pub fn extract_module_attributes_resolved(entity: &Entity) -> HashMap<String, String> {
    let mut attrs = extract_module_attributes(entity);
    let syntax = entity.syntax();

    // (name -> RHS value node) for every `const NAME[: type] = <value>;`.
    // The parser emits a const as a PARAM_DECL node carrying a CONST_KW token
    // (constructor params are PARAM_DECLs too, but live inside a PARAM_LIST —
    // these are the entity's DIRECT children). The first IDENT after CONST_KW
    // is the name; the first node after EQ is the value (a STRUCT_LITERAL for
    // struct consts, possibly wrapped in an expression node).
    let mut consts: HashMap<String, rowan::SyntaxNode<bhdl_ast::BhdlLanguage>> = HashMap::new();
    for child in syntax.children() {
        if child.kind() != SyntaxKind::PARAM_DECL {
            continue;
        }
        let is_const = child
            .children_with_tokens()
            .filter_map(|el| el.into_token())
            .any(|t| t.kind() == SyntaxKind::CONST_KW);
        if !is_const {
            continue;
        }
        let mut name: Option<String> = None;
        let mut saw_eq = false;
        let mut value: Option<rowan::SyntaxNode<bhdl_ast::BhdlLanguage>> = None;
        for el in child.children_with_tokens() {
            match el {
                rowan::NodeOrToken::Token(t) => match t.kind() {
                    SyntaxKind::IDENT if name.is_none() && !saw_eq => {
                        name = Some(t.text().to_string());
                    }
                    SyntaxKind::EQ => saw_eq = true,
                    _ => {}
                },
                rowan::NodeOrToken::Node(n) if saw_eq && value.is_none() => {
                    value = Some(n);
                }
                _ => {}
            }
        }
        if let (Some(n), Some(v)) = (name, value) {
            consts.insert(n, v);
        }
    }

    // ALL constructor param names (with or without a default), plus the
    // subset that have a default value. We need the full set to recognise a
    // reference; the defaults to resolve one.
    let mut param_defaults: HashMap<String, String> = HashMap::new();
    let mut param_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for param_list in syntax.children().filter(|c| c.kind() == SyntaxKind::PARAM_LIST) {
        for pd in param_list.children().filter(|c| c.kind() == SyntaxKind::PARAM_DECL) {
            let mut pname: Option<String> = None;
            let mut saw_eq = false;
            let mut default = String::new();
            for el in pd.children_with_tokens() {
                match el {
                    rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::IDENT && pname.is_none() => {
                        pname = Some(t.text().to_string());
                    }
                    rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::EQ => saw_eq = true,
                    rowan::NodeOrToken::Node(n) if saw_eq => {
                        default.push_str(n.text().to_string().trim());
                    }
                    rowan::NodeOrToken::Token(t)
                        if saw_eq
                            && !matches!(
                                t.kind(),
                                SyntaxKind::WHITESPACE | SyntaxKind::COMMA | SyntaxKind::R_PAREN
                            ) =>
                    {
                        default.push_str(t.text());
                    }
                    _ => {}
                }
            }
            if let Some(p) = pname {
                param_names.insert(p.clone());
                if !default.trim().is_empty() {
                    param_defaults.insert(p, unquote(default.trim()));
                }
            }
        }
    }

    // A value is a REFERENCE only if its head identifier names a constructor
    // param or a `const` — NOT merely because it looks like an identifier. This
    // matters because attribute string values are stored unquoted, so a literal
    // like `attribute component_class = "voltage_regulator"` reads as the bare
    // word `voltage_regulator`; it must be KEPT, while `attribute resistance =
    // value` (the Res entity's un-defaulted `value` param) is a real reference
    // that can't resolve and must be DROPPED — stamping the literal text "value"
    // would make the SPICE converter fall back to a 1kΩ default and corrupt the
    // operating point.
    let is_reference = |text: &str| -> bool {
        let head = text.trim().split('.').next().unwrap_or("");
        !head.is_empty() && (param_names.contains(head) || consts.contains_key(head))
    };
    let mut drop: Vec<String> = Vec::new();
    for (k, v) in attrs.iter_mut() {
        let t = v.trim().to_string();
        if !is_reference(&t) {
            continue; // a literal (string / number / bool / expr) — keep as-is
        }
        match resolve_attr_ref(&t, &consts, &param_defaults) {
            Some(resolved) => *v = resolved,
            None => drop.push(k.clone()), // reference that can't resolve (un-defaulted param)
        }
    }
    for k in drop {
        attrs.remove(&k);
    }
    attrs
}

/// The entity's generic parameters in declaration order:
/// `(name, default-value-text-if-any)`. For
/// `entity LinearRegulator<V_OUT: voltage, HAS_EN: bool = false>` this returns
/// `[("V_OUT", None), ("HAS_EN", Some("false"))]`. Used to bind an alias
/// specialization's arguments (`alias LM7805 = LinearRegulator<5V>;`) so
/// attribute values that reference a generic (`attribute output_voltage =
/// V_OUT`) can be substituted with the concrete argument.
pub fn extract_generic_param_info(entity: &Entity) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for gp_list in entity
        .syntax()
        .children()
        .filter(|c| c.kind() == SyntaxKind::GENERIC_PARAMS)
    {
        for gp in gp_list
            .children()
            .filter(|c| c.kind() == SyntaxKind::GENERIC_PARAM)
        {
            // GENERIC_PARAM = IDENT name [: IDENT type] [= value]. The name is
            // the FIRST IDENT; a type IDENT follows the COLON; the default is
            // everything after EQ.
            let mut name: Option<String> = None;
            let mut saw_eq = false;
            let mut default = String::new();
            for el in gp.children_with_tokens() {
                match el {
                    rowan::NodeOrToken::Token(t) => match t.kind() {
                        SyntaxKind::IDENT if name.is_none() => {
                            name = Some(t.text().to_string());
                        }
                        SyntaxKind::EQ => saw_eq = true,
                        k if saw_eq && !matches!(k, SyntaxKind::WHITESPACE) => {
                            default.push_str(t.text());
                        }
                        _ => {}
                    },
                    rowan::NodeOrToken::Node(n) if saw_eq => {
                        default.push_str(n.text().to_string().trim());
                    }
                    _ => {}
                }
            }
            if let Some(n) = name {
                let d = default.trim();
                out.push((n, if d.is_empty() { None } else { Some(d.to_string()) }));
            }
        }
    }
    out
}

/// Substitute generic-parameter references in attribute VALUES with concrete
/// bindings. A value participates only when it is EXACTLY a generic parameter
/// name (the `attribute output_voltage = V_OUT` shape). Bound → replaced;
/// declared-but-unbound (no argument, no default) → dropped, mirroring the
/// unresolvable-reference policy above (never leave literal identifier text
/// where downstream expects a number).
pub fn substitute_generic_attr_refs(
    attrs: &mut HashMap<String, String>,
    generic_params: &[(String, Option<String>)],
    args: &[String],
) {
    let mut bindings: HashMap<&str, String> = HashMap::new();
    let mut declared: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (i, (name, default)) in generic_params.iter().enumerate() {
        declared.insert(name.as_str());
        if let Some(v) = args.get(i).cloned().or_else(|| default.clone()) {
            bindings.insert(name.as_str(), v);
        }
    }
    let mut drop: Vec<String> = Vec::new();
    for (k, v) in attrs.iter_mut() {
        let t = v.trim();
        if let Some(bound) = bindings.get(t) {
            *v = bound.clone();
        } else if declared.contains(t) {
            drop.push(k.clone());
        }
    }
    for k in drop {
        attrs.remove(&k);
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Resolve a single attribute value that is a bare reference — either a
/// constructor-param name, a `const` name, or a dotted `CONST.field[.field…]`
/// path into a `const`'s struct literal. Returns `None` when the text isn't
/// such a reference or can't be resolved (leaving the literal untouched).
fn resolve_attr_ref(
    text: &str,
    consts: &HashMap<String, rowan::SyntaxNode<bhdl_ast::BhdlLanguage>>,
    param_defaults: &HashMap<String, String>,
) -> Option<String> {
    let text = text.trim();
    // Only resolve things that look like a (possibly dotted) identifier path;
    // never touch numbers, quoted strings, booleans or expressions.
    if text.is_empty()
        || !text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        || text.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true)
    {
        return None;
    }
    let mut parts = text.split('.');
    let head = parts.next()?;
    if text.find('.').is_none() {
        // Bare name: param default first, then a scalar const.
        if let Some(d) = param_defaults.get(head) {
            return Some(d.clone());
        }
        if let Some(node) = consts.get(head) {
            // Only a scalar (non-struct) const resolves to a value text.
            if node.kind() != SyntaxKind::STRUCT_LITERAL {
                return Some(unquote(node.text().to_string().trim()));
            }
        }
        return None;
    }
    // Dotted path: walk struct-literal fields starting from the const.
    let mut cur = as_struct_literal(consts.get(head)?)?;
    let fields: Vec<&str> = parts.collect();
    for (i, field) in fields.iter().enumerate() {
        let last = i + 1 == fields.len();
        match struct_field_value(&cur, field)? {
            rowan::NodeOrToken::Token(t) => return Some(unquote(t.text().trim())),
            rowan::NodeOrToken::Node(n) => {
                if last {
                    return Some(unquote(n.text().to_string().trim()));
                }
                cur = as_struct_literal(&n)?;
            }
        }
    }
    None
}

/// Coerce a node to the STRUCT_LITERAL it is or directly wraps. `parse_const_decl`
/// runs the RHS through `parse_expression`, which may wrap the `{ … }` in an
/// expression node, so accept the first STRUCT_LITERAL in pre-order.
fn as_struct_literal(
    node: &rowan::SyntaxNode<bhdl_ast::BhdlLanguage>,
) -> Option<rowan::SyntaxNode<bhdl_ast::BhdlLanguage>> {
    if node.kind() == SyntaxKind::STRUCT_LITERAL {
        return Some(node.clone());
    }
    node.descendants().find(|d| d.kind() == SyntaxKind::STRUCT_LITERAL)
}

/// Find the value following `field:` inside a STRUCT_LITERAL node. The parser
/// emits field-name IDENT tokens and value expressions inline (no wrapper
/// node), so scan for the matching name, then the COLON, then the next
/// non-trivia element is the value.
fn struct_field_value(
    struct_lit: &rowan::SyntaxNode<bhdl_ast::BhdlLanguage>,
    field: &str,
) -> Option<rowan::NodeOrToken<rowan::SyntaxNode<bhdl_ast::BhdlLanguage>, rowan::SyntaxToken<bhdl_ast::BhdlLanguage>>> {
    let mut depth = 0i32; // brace depth, to stay at this struct's top level
    let mut matched = false;
    let mut after_colon = false;
    for el in struct_lit.children_with_tokens() {
        match &el {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::L_BRACE => depth += 1,
                SyntaxKind::R_BRACE => depth -= 1,
                SyntaxKind::WHITESPACE | SyntaxKind::COMMENT => {}
                SyntaxKind::IDENT if depth == 1 && !matched => {
                    matched = t.text() == field;
                }
                SyntaxKind::COLON if matched => after_colon = true,
                _ if after_colon => return Some(el.clone()),
                _ => {}
            },
            rowan::NodeOrToken::Node(_) if after_colon => return Some(el.clone()),
            _ => {}
        }
    }
    None
}

/// Extract a cleaned value from an expression node
fn extract_node_value(node: &rowan::SyntaxNode<bhdl_ast::BhdlLanguage>) -> String {
    let text = node.text().to_string().trim().to_string();
    // Remove surrounding quotes from string literals
    if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        text[1..text.len()-1].to_string()
    } else {
        text
    }
}

/// Extract attributes from a board's syntax tree
pub fn extract_board_attributes(board: &Board) -> HashMap<String, String> {
    let mut attributes = HashMap::new();

    let syntax = board.syntax();

    for child in syntax.children() {
        if child.kind() == SyntaxKind::ATTRIBUTE_DECL {
            let mut name: Option<String> = None;
            let mut found_eq = false;

            for elem in child.children_with_tokens() {
                match elem {
                    rowan::NodeOrToken::Token(token) => {
                        match token.kind() {
                            SyntaxKind::ATTRIBUTE_KW => {}
                            SyntaxKind::IDENT if name.is_none() => {
                                name = Some(token.text().to_string());
                            }
                            SyntaxKind::EQ => {
                                found_eq = true;
                            }
                            SyntaxKind::SEMI | SyntaxKind::WHITESPACE => {}
                            _ if found_eq && name.is_some() => {
                                let value = token.text().to_string();
                                if let Some(attr_name) = name.take() {
                                    attributes.insert(attr_name, value.trim().to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                    rowan::NodeOrToken::Node(node) if found_eq && name.is_some() => {
                        let value = extract_node_value(&node);
                        if let Some(attr_name) = name.take() {
                            attributes.insert(attr_name, value);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    attributes
}

/// Substitute generic parameter references in attribute values with concrete values.
///
/// For example, if an entity has `attribute output_voltage = V_OUT;` and
/// `V_OUT` was specialized to `Voltage(5.0)`, this replaces the attribute
/// value "V_OUT" with "5" (the numeric representation).
pub fn substitute_generic_params(
    attrs: &mut HashMap<String, String>,
    concrete_params: &std::collections::BTreeMap<String, bhdl_common::ConstValue>,
) {
    for (_attr_name, attr_value) in attrs.iter_mut() {
        let trimmed = attr_value.trim();
        // Check if the attribute value matches a generic param name
        if let Some(cv) = concrete_params.get(trimmed) {
            *attr_value = const_value_to_attr_string(cv);
        }
    }
}

/// Convert a ConstValue to a string suitable for attribute values.
/// Returns the raw numeric value (in base SI units) for physical quantities.
fn const_value_to_attr_string(cv: &bhdl_common::ConstValue) -> String {
    match cv {
        bhdl_common::ConstValue::Integer(n) => format!("{}", n),
        bhdl_common::ConstValue::Float(f) => format_f64(*f),
        bhdl_common::ConstValue::Bool(b) => format!("{}", b),
        bhdl_common::ConstValue::String(s) => s.clone(),
        bhdl_common::ConstValue::Voltage(v) => format_f64(*v),
        bhdl_common::ConstValue::Current(a) => format_f64(*a),
        bhdl_common::ConstValue::Resistance(r) => format_f64(*r),
        bhdl_common::ConstValue::Capacitance(c) => format_f64(*c),
        bhdl_common::ConstValue::Inductance(l) => format_f64(*l),
        bhdl_common::ConstValue::Power(w) => format_f64(*w),
        bhdl_common::ConstValue::Frequency(hz) => format_f64(*hz),
        bhdl_common::ConstValue::Time(t) => format_f64(*t),
    }
}

/// Format an f64, dropping the decimal if it's an integer value.
fn format_f64(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

/// Extract attributes from a component instance in the AST
pub fn extract_component_attributes(component_name: &str, params: &HashMap<String, String>) -> HashMap<String, String> {
    // For component instances like LED(red), Res(10k), etc.
    // We need to map the parameters to expected attribute names

    let mut attributes = HashMap::new();

    match component_name {
        "LED" => {
            if let Some(color) = params.get("color").or_else(|| params.values().next()) {
                attributes.insert("color".to_string(), color.clone());
            }
        }
        "Res" | "Resistor" => {
            if let Some(value) = params.get("value").or_else(|| params.values().next()) {
                attributes.insert("value".to_string(), value.clone());
            }
        }
        "Cap" | "Capacitor" => {
            if let Some(value) = params.get("value").or_else(|| params.values().next()) {
                attributes.insert("value".to_string(), value.clone());
            }
        }
        _ => {
            attributes.extend(params.clone());
        }
    }

    attributes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_attributes_from_entity() {
        // Parse a simple entity with attributes
        let source = r#"
entity Res(value: resistance) {
    pin 1: signal inout;
    pin 2: signal inout;
    attribute component_class = "resistor";
    attribute tolerance = 0.05;
}
"#;
        let parse = bhdl_parser::parse(source);
        let source_file = bhdl_ast::SourceFile::cast(parse.syntax()).unwrap();

        let entities: Vec<_> = source_file.entities().collect();
        assert_eq!(entities.len(), 1);

        let attrs = extract_module_attributes(&entities[0]);
        assert_eq!(attrs.get("component_class"), Some(&"resistor".to_string()));
    }

    #[test]
    fn resolves_param_and_const_references() {
        // `attribute f_sw = f_sw;`           -> param default
        // `attribute sf = P.switching_frequency;` -> const struct field
        // `attribute oi = P.impedance.output_impedance;` -> nested field
        // `attribute cc = "voltage_regulator";`   -> literal, untouched
        let source = r#"
entity Reg(f_sw: frequency = 570kHz) {
    pin VIN: power in;
    const P: T = {
        output_current: 3A,
        switching_frequency: 500kHz,
        impedance: { output_impedance: 0.05 },
    };
    attribute component_class = "voltage_regulator";
    attribute f_sw = f_sw;
    attribute sf = P.switching_frequency;
    attribute oc = P.output_current;
    attribute oi = P.impedance.output_impedance;
}
"#;
        let parse = bhdl_parser::parse(source);
        let sf = bhdl_ast::SourceFile::cast(parse.syntax()).unwrap();
        let entity = sf.entities().next().unwrap();
        let attrs = extract_module_attributes_resolved(&entity);
        assert_eq!(attrs.get("component_class"), Some(&"voltage_regulator".to_string()));
        assert_eq!(attrs.get("f_sw"), Some(&"570kHz".to_string()));
        assert_eq!(attrs.get("sf"), Some(&"500kHz".to_string()));
        assert_eq!(attrs.get("oc"), Some(&"3A".to_string()));
        assert_eq!(attrs.get("oi"), Some(&"0.05".to_string()));
    }
}
