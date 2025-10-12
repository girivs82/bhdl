// Debug program to understand how hierarchical paths are parsed

use bhdl_parser;
use bhdl_ast::{AstNode, SourceFile};

fn main() {
    let source = std::fs::read_to_string("tests/circuits/realistic/test_hierarchical_wildcard.bhdl")
        .expect("Failed to read test file");

    let parse = bhdl_parser::parse(&source);
    let ast = SourceFile::cast(parse.syntax()).expect("Failed to cast to SourceFile");

    println!("=== Analyzing Distribution Pin Lists ===\n");

    for item in ast.items() {
        if let Some(board) = bhdl_ast::Board::cast(item.syntax().clone()) {
            for power_domain in board.power_domains() {
                if let Some(distribution) = power_domain.distribution_block() {
                    for pin_list in distribution.pin_lists() {
                        let component = pin_list.component().unwrap_or_else(|| "None".to_string());
                        let pin_name = pin_list.pin_name().unwrap_or_else(|| "None".to_string());
                        let has_wildcard = pin_list.has_wildcard();
                        let full_text = pin_list.syntax().text().to_string();

                        println!("Distribution pin list:");
                        println!("  Full text: {}", full_text.trim());
                        println!("  component(): {}", component);
                        println!("  pin_name(): {}", pin_name);
                        println!("  has_wildcard(): {}", has_wildcard);
                        println!("  Dot count: {}", full_text.matches('.').count());
                        println!();
                    }
                }
            }
        }
    }
}
