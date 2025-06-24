//! BHDL Expression Evaluator
//! 
//! This module provides expression evaluation for BHDL stdlib attributes
//! that reference parameters and constants. This is used by multiple crates
//! including bhdl-spice, bhdl-synthesizer, and bhdl-analyzer.

use std::collections::HashMap;
use anyhow::{Result, Context as _};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Module, AttributeDecl, HasName};
use bhdl_ast::expr::Expr;

/// Represents a value in the expression evaluator
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    String(String),
    Bool(bool),
    Struct(HashMap<String, Value>),
    Null,
}

impl Value {
    /// Try to convert to f64
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Try to convert to string
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    
    /// Try to convert to bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Expression evaluator with symbol table
pub struct ExpressionEvaluator {
    /// Symbol table mapping names to values
    symbols: HashMap<String, Value>,
    /// Cache of evaluated modules
    module_cache: HashMap<String, HashMap<String, Value>>,
}

impl ExpressionEvaluator {
    /// Create a new expression evaluator
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
            module_cache: HashMap::new(),
        }
    }
    
    /// Check if a symbol exists
    pub fn has_symbol(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }
    
    /// Load a module and extract its constants
    pub fn load_module(&mut self, module_name: &str, module_content: &str) -> Result<()> {
        // Parse the module
        let parse_result = parse(module_content);
        let syntax_node = parse_result.syntax();
        let source_file = SourceFile::cast(syntax_node)
            .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
        
        // Find the module
        for item in source_file.items() {
            if let Some(module) = Module::cast(item.syntax().clone()) {
                if let Some(name) = module.name() {
                    if name.text() == module_name {
                        // Extract constants from this module
                        let constants = self.extract_module_constants(&module)?;
                        self.module_cache.insert(module_name.to_string(), constants.clone());
                        
                        // Also add to global symbols for easy access
                        for (key, value) in constants {
                            self.symbols.insert(key, value);
                        }
                        
                        // Now handle any constant references like "const params = LM7805_PARAMS"
                        self.resolve_const_references(&module)?;
                        
                        return Ok(());
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("Module {} not found", module_name))
    }
    
    /// Extract constants from a module
    fn extract_module_constants(&self, module: &Module) -> Result<HashMap<String, Value>> {
        let mut constants = HashMap::new();
        
        // Since the parser uses PARAM_DECL for const declarations,
        // we need to look at the raw syntax tree
        let syntax = module.syntax();
        
        // Iterate through all children looking for const declarations
        for child in syntax.children() {
            // Check if this is a const declaration by looking for CONST_KW
            let text = child.text().to_string();
            if text.trim().starts_with("const ") {
                // Parse the const declaration manually
                if let Some((const_name, const_value)) = self.parse_const_declaration(&text) {
                    constants.insert(const_name.clone(), const_value);
                    
                    // Special handling: if this is a params assignment, also add it as "params"
                    if const_name == "params" {
                        // Already added
                    }
                }
            }
        }
        
        Ok(constants)
    }
    
    /// Parse a const declaration from text
    fn parse_const_declaration(&self, text: &str) -> Option<(String, Value)> {
        // Format: const NAME: TYPE = VALUE; or const NAME = VALUE;
        let text = text.trim();
        if !text.starts_with("const ") {
            return None;
        }
        
        // Find the name
        let after_const = &text[6..];
        
        // Look for either : or = to find the end of the name
        let name_end = after_const.find(':').or_else(|| after_const.find('='))?;
        let name = after_const[..name_end].trim().to_string();
        
        // Find the equals sign
        if let Some(eq_pos) = after_const.find('=') {
            let value_part = &after_const[eq_pos + 1..];
            // Remove trailing semicolon if present
            let value_part = value_part.trim_end_matches(';').trim();
            
            // Check if this is a reference to another constant
            if value_part.chars().all(|c| c.is_alphanumeric() || c == '_') && !value_part.chars().next().unwrap_or('0').is_numeric() {
                // This is likely a reference to another constant
                // For now, we'll need to resolve this later
                return None;
            }
            
            // Parse the value
            if let Ok(value) = self.parse_const_value(value_part) {
                return Some((name, value));
            }
        }
        
        None
    }
    
    /// Resolve constant references in a module
    fn resolve_const_references(&mut self, module: &Module) -> Result<()> {
        let syntax = module.syntax();
        
        // Look for const declarations that reference other constants
        for child in syntax.children() {
            let text = child.text().to_string();
            if text.trim().starts_with("const ") {
                // Try to parse it
                let text = text.trim();
                let after_const = &text[6..];
                
                // Look for either : or = to find the end of the name
                if let Some(name_end) = after_const.find(':').or_else(|| after_const.find('=')) {
                    let name = after_const[..name_end].trim();
                    
                    if let Some(eq_pos) = after_const.find('=') {
                        let value_part = &after_const[eq_pos + 1..].trim_end_matches(';').trim();
                        
                        // Check if this is a reference to another constant
                        if value_part.chars().all(|c| c.is_alphanumeric() || c == '_') && !value_part.chars().next().unwrap_or('0').is_numeric() {
                            // Look up the referenced constant
                            if let Some(referenced_value) = self.symbols.get(&value_part.to_string()).cloned() {
                                self.symbols.insert(name.to_string(), referenced_value);
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Parse a const value which could be a struct literal or a simple value
    fn parse_const_value(&self, value_str: &str) -> Result<Value> {
        let value_str = value_str.trim();
        
        if value_str.starts_with('{') {
            // This is a struct literal
            self.parse_struct_literal(value_str)
        } else {
            // Try to parse as a simple value
            self.parse_field_value(value_str)
        }
    }
    
    /// Evaluate an expression
    pub fn evaluate_expr(&self, expr: &Expr, local_symbols: &HashMap<String, Value>) -> Result<Value> {
        // For now, just get the text and evaluate it
        let text = expr.syntax().text().to_string();
        self.evaluate_string(&text, local_symbols)
    }
    
    
    /// Evaluate a path expression (e.g., params.output_voltage)
    pub fn evaluate_path(&self, path: &str, local_symbols: &HashMap<String, Value>) -> Result<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        
        if parts.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }
        
        // Start with the base identifier
        let base = parts[0];
        let mut current_value = if let Some(val) = local_symbols.get(base) {
            val.clone()
        } else if let Some(val) = self.symbols.get(base) {
            val.clone()
        } else {
            return Err(anyhow::anyhow!("Unknown identifier: {}", base));
        };
        
        // Follow the field accesses
        for field in &parts[1..] {
            match &current_value {
                Value::Struct(fields) => {
                    if let Some(field_value) = fields.get(*field) {
                        current_value = field_value.clone();
                    } else {
                        return Err(anyhow::anyhow!("Field {} not found in struct", field));
                    }
                },
                _ => return Err(anyhow::anyhow!("Cannot access field {} on non-struct value", field)),
            }
        }
        
        Ok(current_value)
    }
    
    
    /// Parse a struct literal from text
    fn parse_struct_literal(&self, text: &str) -> Result<Value> {
        let text = text.trim();
        if !text.starts_with('{') || !text.ends_with('}') {
            return Err(anyhow::anyhow!("Invalid struct literal format"));
        }
        
        let mut fields = HashMap::new();
        let content = &text[1..text.len()-1];
        
        // State machine for parsing
        let mut current_field = String::new();
        let mut current_value = String::new();
        let mut in_field = true;
        let mut depth = 0;
        let mut in_string = false;
        let mut prev_char = ' ';
        
        for ch in content.chars() {
            if in_string {
                current_value.push(ch);
                if ch == '"' && prev_char != '\\' {
                    in_string = false;
                }
            } else if ch == '"' {
                in_string = true;
                current_value.push(ch);
            } else if ch == '{' {
                depth += 1;
                current_value.push(ch);
            } else if ch == '}' {
                depth -= 1;
                current_value.push(ch);
            } else if depth == 0 && ch == ':' && in_field {
                in_field = false;
            } else if depth == 0 && ch == ',' {
                // End of field-value pair
                let field_name = current_field.trim().to_string();
                let field_value = self.parse_field_value(current_value.trim())?;
                fields.insert(field_name, field_value);
                
                current_field.clear();
                current_value.clear();
                in_field = true;
            } else if in_field {
                current_field.push(ch);
            } else {
                current_value.push(ch);
            }
            prev_char = ch;
        }
        
        // Handle last field
        if !current_field.is_empty() {
            let field_name = current_field.trim().to_string();
            let field_value = self.parse_field_value(current_value.trim())?;
            fields.insert(field_name, field_value);
        }
        
        Ok(Value::Struct(fields))
    }
    
    /// Parse a field value which could be a literal, another struct, or null
    fn parse_field_value(&self, value_str: &str) -> Result<Value> {
        let value_str = value_str.trim();
        
        // Check for struct literal
        if value_str.starts_with('{') && value_str.ends_with('}') {
            return self.parse_struct_literal(value_str);
        }
        
        // Check for null
        if value_str == "null" {
            return Ok(Value::Null);
        }
        
        // Check for boolean
        if value_str == "true" {
            return Ok(Value::Bool(true));
        } else if value_str == "false" {
            return Ok(Value::Bool(false));
        }
        
        // Check for string literal
        if value_str.starts_with('"') && value_str.ends_with('"') {
            return Ok(Value::String(value_str[1..value_str.len()-1].to_string()));
        }
        
        // Try to parse as electrical value (number with units)
        if let Ok(num) = self.parse_electrical_value(value_str) {
            return Ok(Value::Number(num));
        }
        
        // Otherwise, it might be a bare number
        if let Ok(num) = value_str.parse::<f64>() {
            return Ok(Value::Number(num));
        }
        
        // If all else fails, treat as string
        Ok(Value::String(value_str.to_string()))
    }
    
    /// Parse electrical value with units
    pub fn parse_electrical_value(&self, value_str: &str) -> Result<f64> {
        let value_str = value_str.trim();
        
        // Check for percentage
        if value_str.ends_with('%') {
            let numeric = value_str.trim_end_matches('%');
            return numeric.parse::<f64>()
                .map(|v| v / 100.0)
                .context("Failed to parse percentage");
        }
        
        // Find where the unit starts
        let mut numeric_end = value_str.len();
        for (i, ch) in value_str.char_indices() {
            if ch.is_alphabetic() || ch == '°' || ch == 'Ω' || ch == 'µ' {
                numeric_end = i;
                break;
            }
        }
        
        let numeric_part = &value_str[..numeric_end];
        let unit_part = &value_str[numeric_end..];
        
        let base_value = numeric_part.parse::<f64>()
            .with_context(|| format!("Failed to parse numeric part: {}", numeric_part))?;
        
        // Apply unit multiplier
        let multiplier = self.get_unit_multiplier(unit_part)?;
        Ok(base_value * multiplier)
    }
    
    /// Get multiplier for electrical units
    fn get_unit_multiplier(&self, unit: &str) -> Result<f64> {
        match unit {
            // Voltage
            "V" => Ok(1.0),
            "mV" => Ok(1e-3),
            "kV" => Ok(1e3),
            "µV" | "uV" => Ok(1e-6),
            
            // Current
            "A" => Ok(1.0),
            "mA" => Ok(1e-3),
            "µA" | "uA" => Ok(1e-6),
            "nA" => Ok(1e-9),
            
            // Resistance
            "Ω" | "ohm" => Ok(1.0),
            "kΩ" | "kohm" => Ok(1e3),
            "MΩ" | "Mohm" => Ok(1e6),
            "mΩ" | "mohm" => Ok(1e-3),
            
            // Capacitance
            "F" => Ok(1.0),
            "mF" => Ok(1e-3),
            "µF" | "uF" => Ok(1e-6),
            "nF" => Ok(1e-9),
            "pF" => Ok(1e-12),
            
            // Inductance
            "H" => Ok(1.0),
            "mH" => Ok(1e-3),
            "µH" | "uH" => Ok(1e-6),
            "nH" => Ok(1e-9),
            
            // Time
            "s" => Ok(1.0),
            "ms" => Ok(1e-3),
            "µs" | "us" => Ok(1e-6),
            "ns" => Ok(1e-9),
            "ps" => Ok(1e-12),
            
            // Frequency
            "Hz" => Ok(1.0),
            "kHz" => Ok(1e3),
            "MHz" => Ok(1e6),
            "GHz" => Ok(1e9),
            
            // Power
            "W" => Ok(1.0),
            "mW" => Ok(1e-3),
            "kW" => Ok(1e3),
            
            // Temperature
            "°C" | "C" => Ok(1.0), // Direct Celsius value
            
            // Dimensionless or unknown
            "" => Ok(1.0),
            "dB" => Ok(1.0), // dB is already a ratio
            
            _ => Err(anyhow::anyhow!("Unknown unit: {}", unit)),
        }
    }
    
    /// Evaluate an expression string directly
    pub fn evaluate_string(&self, expr_str: &str, local_symbols: &HashMap<String, Value>) -> Result<Value> {
        // First try to parse as a literal value
        if let Ok(value) = self.parse_electrical_value(expr_str) {
            return Ok(Value::Number(value));
        }
        
        // Check for boolean literals
        if expr_str == "true" {
            return Ok(Value::Bool(true));
        } else if expr_str == "false" {
            return Ok(Value::Bool(false));
        } else if expr_str == "null" {
            return Ok(Value::Null);
        }
        
        // Check if it's a string literal
        if expr_str.starts_with('"') && expr_str.ends_with('"') {
            return Ok(Value::String(expr_str[1..expr_str.len()-1].to_string()));
        }
        
        // Otherwise, try to evaluate as a path expression
        self.evaluate_path(expr_str, local_symbols)
    }
}

impl Default for ExpressionEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_electrical_values() {
        let evaluator = ExpressionEvaluator::new();
        
        // Voltage tests
        assert_eq!(evaluator.parse_electrical_value("5V").unwrap(), 5.0);
        assert_eq!(evaluator.parse_electrical_value("3.3V").unwrap(), 3.3);
        assert_eq!(evaluator.parse_electrical_value("12mV").unwrap(), 0.012);
        assert_eq!(evaluator.parse_electrical_value("1.5kV").unwrap(), 1500.0);
        
        // Current tests
        assert_eq!(evaluator.parse_electrical_value("20mA").unwrap(), 0.02);
        assert_eq!(evaluator.parse_electrical_value("5µA").unwrap(), 5e-6);
        
        // Resistance tests
        assert_eq!(evaluator.parse_electrical_value("10kΩ").unwrap(), 10000.0);
        assert_eq!(evaluator.parse_electrical_value("0.03Ω").unwrap(), 0.03);
        
        // Percentage tests
        assert_eq!(evaluator.parse_electrical_value("0.01%").unwrap(), 0.0001);
        assert_eq!(evaluator.parse_electrical_value("4%").unwrap(), 0.04);
        
        // Temperature tests
        assert_eq!(evaluator.parse_electrical_value("125°C").unwrap(), 125.0);
    }
    
    #[test]
    fn test_evaluate_path() {
        let mut evaluator = ExpressionEvaluator::new();
        
        // Add test symbols
        let mut impedance_struct = HashMap::new();
        impedance_struct.insert("output_impedance".to_string(), Value::Number(0.03));
        impedance_struct.insert("voltage_drop".to_string(), Value::Number(2.0));
        
        let mut params_struct = HashMap::new();
        params_struct.insert("output_voltage".to_string(), Value::Number(5.0));
        params_struct.insert("impedance".to_string(), Value::Struct(impedance_struct));
        
        evaluator.symbols.insert("params".to_string(), Value::Struct(params_struct));
        
        // Test simple path
        let result = evaluator.evaluate_path("params.output_voltage", &HashMap::new()).unwrap();
        assert_eq!(result.as_f64().unwrap(), 5.0);
        
        // Test nested path
        let result = evaluator.evaluate_path("params.impedance.output_impedance", &HashMap::new()).unwrap();
        assert_eq!(result.as_f64().unwrap(), 0.03);
    }
}