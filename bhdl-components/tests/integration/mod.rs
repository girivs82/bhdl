//! Integration tests for BHDL Components
//! 
//! These tests validate the complete component intelligence pipeline:
//! - KiCad library import and parsing
//! - Component database operations
//! - Supplier API integration with caching
//! - Two-stage component synthesis
//! - Alternative component selection

pub mod real_world_test;

use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize test environment
pub fn init_test_env() {
    INIT.call_once(|| {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .is_test(true)
            .try_init()
            .ok();
        
        println!("🧪 BHDL Components Integration Test Suite");
        println!("==========================================");
    });
}

/// Check if supplier APIs are configured for testing
pub fn check_supplier_apis() -> (bool, Vec<String>) {
    let mut available = Vec::new();
    let mut has_any = false;
    
    if std::env::var("DIGIKEY_CLIENT_ID").is_ok() && std::env::var("DIGIKEY_CLIENT_SECRET").is_ok() {
        available.push("DigiKey".to_string());
        has_any = true;
    }
    
    if std::env::var("NEXAR_CLIENT_ID").is_ok() && std::env::var("NEXAR_CLIENT_SECRET").is_ok() {
        available.push("Nexar".to_string());
        has_any = true;
    }
    
    (has_any, available)
}

/// Print test environment information
pub fn print_test_environment() {
    let (has_apis, available_apis) = check_supplier_apis();
    
    println!("🔧 Test Environment:");
    if has_apis {
        println!("   ✅ Supplier APIs: {}", available_apis.join(", "));
    } else {
        println!("   ⚠️  No supplier APIs configured");
        println!("      Set DIGIKEY_CLIENT_ID/SECRET or NEXAR_CLIENT_ID/SECRET for full tests");
    }
    
    // Check for KiCad libraries
    if let Ok(libs) = self::real_world_test::find_kicad_libraries() {
        if !libs.is_empty() {
            println!("   ✅ KiCad libraries: {} found", libs.len());
        } else {
            println!("   ⚠️  No KiCad libraries found");
        }
    }
    
    println!();
}