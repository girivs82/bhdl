use bhdl_parser;
use bhdl_ast::{AstNode, SourceFile};

fn main() {
    let source = r#"
board Test {
    power_domain @VCC = 3.3V @ 1A {
        distribution {
            sensor_board[*].sensor.VCC;
            array.*sensor.VCC;
            led.A;
        }
    }
    ground GND;
}
"#;

    let parse = bhdl_parser::parse(source);
    let ast = SourceFile::cast(parse.syntax()).expect("Failed to cast");

    for item in ast.items() {
        if let Some(board) = bhdl_ast::Board::cast(item.syntax().clone()) {
            for pd in board.power_domains() {
                if let Some(dist) = pd.distribution_block() {
                    for pin_list in dist.pin_lists() {
                        let segments = pin_list.path_segments();
                        let is_hier = pin_list.is_hierarchical();
                        let full = pin_list.full_path();
                        println!("Path: {} | Hierarchical: {} | Segments: {:?}", full, is_hier, segments);
                    }
                }
            }
        }
    }
}
