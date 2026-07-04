use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_ast::items::PatternType;
use std::fs;

fn main() {
    let test_file = std::env::args().nth(1)
        .unwrap_or_else(|| "tests/circuits/realistic/test_advanced_patterns.bhdl".to_string());

    println!("Testing Advanced Pattern Matching Parser");
    println!("========================================\n");
    println!("Reading file: {}\n", test_file);

    let source = fs::read_to_string(&test_file)
        .expect("Failed to read test file");

    let parse_result = parse(&source);

    if !parse_result.errors().is_empty() {
        println!("❌ PARSE ERRORS:");
        for error in parse_result.errors() {
            println!("  {:?}", error);
        }
        return;
    }

    let source_file = SourceFile::cast(parse_result.syntax()).expect("Failed to cast to SourceFile");

    println!("✅ File parsed successfully\n");
    println!("Testing Pattern Classification");
    println!("==============================\n");

    let mut test_count = 0;
    let mut pass_count = 0;

    // Find all power domains and test their patterns
    for board in source_file.boards() {
        for domain in board.power_domains() {
            let domain_name = domain.net_name()
                .map(|n| format!("@{}", n))
                .unwrap_or_else(|| "unnamed".to_string());

            println!("Power Domain: {}", domain_name);

            let dist_block = match domain.distribution_block() {
                Some(block) => block,
                None => continue,
            };

            for pin_list in dist_block.pin_lists() {
                let full_path = pin_list.full_path();
                let pattern_type = pin_list.pattern_type();
                let params = pin_list.pattern_params();

                test_count += 1;

                println!("  Path: {}", full_path);
                println!("  Pattern Type: {:?}", pattern_type);

                // Verify expected pattern types based on domain name
                let expected_passes = match domain_name.as_str() {
                    "@VCC_EVEN" => {
                        if matches!(pattern_type, PatternType::EvenKeyword) {
                            println!("  ✅ Correctly identified as EvenKeyword");
                            true
                        } else {
                            println!("  ❌ Expected EvenKeyword, got {:?}", pattern_type);
                            false
                        }
                    }
                    "@VCC_ODD" => {
                        if matches!(pattern_type, PatternType::OddKeyword) {
                            println!("  ✅ Correctly identified as OddKeyword");
                            true
                        } else {
                            println!("  ❌ Expected OddKeyword, got {:?}", pattern_type);
                            false
                        }
                    }
                    "@VCC_SPECIAL" => {
                        if let PatternType::ExplicitList(indices) = pattern_type {
                            let expected = vec![0, 5, 10, 15];
                            if indices == expected {
                                println!("  ✅ Correctly identified as ExplicitList with indices: {:?}", indices);
                                true
                            } else {
                                println!("  ❌ Expected indices {:?}, got {:?}", expected, indices);
                                false
                            }
                        } else {
                            println!("  ❌ Expected ExplicitList, got {:?}", pattern_type);
                            false
                        }
                    }
                    "@VCC_SAMPLED" => {
                        if let PatternType::SteppedRange(start, end, step) = pattern_type {
                            if start == 0 && end == 15 && step == 3 {
                                println!("  ✅ Correctly identified as SteppedRange(0, 15, 3)");
                                println!("  Computed indices: {:?}", params.indices);
                                true
                            } else {
                                println!("  ❌ Expected SteppedRange(0, 15, 3), got ({}, {}, {})", start, end, step);
                                false
                            }
                        } else {
                            println!("  ❌ Expected SteppedRange, got {:?}", pattern_type);
                            false
                        }
                    }
                    "@VCC_RANGE" => {
                        if let PatternType::SimpleRange(start, end) = pattern_type {
                            if start == 0 && end == 4 {
                                println!("  ✅ Correctly identified as SimpleRange(0, 4)");
                                println!("  Computed indices: {:?}", params.indices);
                                true
                            } else {
                                println!("  ❌ Expected SimpleRange(0, 4), got ({}, {})", start, end);
                                false
                            }
                        } else {
                            println!("  ❌ Expected SimpleRange, got {:?}", pattern_type);
                            false
                        }
                    }
                    "@VCC_SINGLE" => {
                        if let PatternType::ExplicitList(indices) = pattern_type {
                            if indices == vec![7] {
                                println!("  ✅ Correctly identified as ExplicitList with single index: 7");
                                true
                            } else {
                                println!("  ❌ Expected indices [7], got {:?}", indices);
                                false
                            }
                        } else {
                            println!("  ❌ Expected ExplicitList, got {:?}", pattern_type);
                            false
                        }
                    }
                    "@VCC_ALL" => {
                        if matches!(pattern_type, PatternType::Wildcard) {
                            println!("  ✅ Correctly identified as Wildcard");
                            true
                        } else {
                            println!("  ❌ Expected Wildcard, got {:?}", pattern_type);
                            false
                        }
                    }
                    _ => {
                        println!("  ⚠️  Unknown domain name");
                        false
                    }
                };

                if expected_passes {
                    pass_count += 1;
                }

                println!();
            }
        }
    }

    println!("========================================");
    println!("Test Results: {}/{} patterns passed", pass_count, test_count);

    if pass_count == test_count {
        println!("✅ All pattern types correctly identified!");
    } else {
        println!("❌ Some pattern types were not correctly identified");
        std::process::exit(1);
    }
}
