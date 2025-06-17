/// Test configuration utilities for BHDL tests
/// 
/// IMPORTANT: All test files must use this configuration to ensure proper organization.
/// Never create test files in the project root directory!
/// 
/// Include this in test binaries to get consistent paths:
/// ```rust
/// #[path = "../../../tests/test_config.rs"]
/// mod test_config;
/// use test_config::*;
/// ```
/// 
/// Example usage:
/// ```rust
/// fn main() {
///     // Input: Get test circuit from organized location
///     let circuit = test_config::circuits::test_7805();
///     
///     // Output: Write to organized output directory
///     let svg_output = test_config::outputs::svg("my_test_result");
///     
///     // Process test...
/// }
/// ```

use std::path::{Path, PathBuf};
use std::env;

/// Get the path to a test circuit file
pub fn test_circuit(category: &str, filename: &str) -> PathBuf {
    let mut path = project_root();
    path.push("tests");
    path.push("circuits");
    path.push(category);
    path.push(filename);
    path
}

/// Get the path for test output files
pub fn test_output(category: &str, filename: &str) -> PathBuf {
    let mut path = project_root();
    path.push("tests");
    path.push("outputs");
    path.push(category);
    
    // Ensure directory exists
    std::fs::create_dir_all(&path).ok();
    
    path.push(filename);
    path
}

/// Get project root directory
pub fn project_root() -> PathBuf {
    let mut path = env::current_dir().expect("Failed to get current directory");
    
    // Walk up until we find Cargo.toml with workspace
    while !path.join("Cargo.toml").exists() || 
          !std::fs::read_to_string(path.join("Cargo.toml"))
           .unwrap_or_default()
           .contains("[workspace]") {
        if !path.pop() {
            panic!("Could not find workspace root");
        }
    }
    
    path
}

/// Standard test circuits
pub mod circuits {
    use super::*;
    
    pub fn simple_led() -> PathBuf {
        test_circuit("simple", "simple_led.bhdl")
    }
    
    pub fn test_7805() -> PathBuf {
        test_circuit("realistic", "test_7805_regulator.bhdl")
    }
    
    pub fn test_7805_realistic() -> PathBuf {
        test_circuit("realistic", "test_7805_regulator_realistic.bhdl")
    }
}

/// Standard output paths
pub mod outputs {
    use super::*;
    
    pub fn svg(name: &str) -> PathBuf {
        test_output("svg", &format!("{}.svg", name))
    }
    
    pub fn netlist(name: &str) -> PathBuf {
        test_output("netlists", &format!("{}.net", name))
    }
}