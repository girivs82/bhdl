use bhdl_spice::model_factory::parse_value;

fn main() {
    let test_values = vec![
        "100n",
        "100nF",
        "10k",
        "10kΩ",
        "4.7k",
        "2.2μF",
        "1000",
        "47pF",
    ];
    
    for val in test_values {
        match parse_value(val) {
            Some(parsed) => println!("{} -> {:.3e}", val, parsed),
            None => println!("{} -> FAILED TO PARSE", val),
        }
    }
}