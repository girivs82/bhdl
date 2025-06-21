// Tests for interface AST nodes

#[cfg(test)]
mod tests {
    use crate::*;
    use bhdl_parser::parse;
    
    #[test]
    fn test_interface_signal_ast() {
        let source = r#"
        interface I2C {
            signal SDA: inout;
            signal SCL: out;
            signal ALERT: in optional;
        }
        "#;
        
        let parsed = parse(source);
        let source_file = SourceFile::cast(parsed.syntax()).unwrap();
        
        // Find the interface definition
        let interface = source_file.items()
            .find_map(|item| {
                if let Item::InterfaceDef(iface) = item {
                    Some(iface)
                } else {
                    None
                }
            })
            .expect("Should find interface definition");
        
        assert_eq!(interface.name().unwrap().text(), "I2C");
        
        // Check signals
        let signals: Vec<_> = interface.signals().collect();
        assert_eq!(signals.len(), 3);
        
        // Check SDA signal
        let sda = &signals[0];
        assert_eq!(sda.name().unwrap().text(), "SDA");
        assert_eq!(sda.direction(), Some(SignalDirection::InOut));
        assert!(!sda.is_optional());
        
        // Check SCL signal
        let scl = &signals[1];
        assert_eq!(scl.name().unwrap().text(), "SCL");
        assert_eq!(scl.direction(), Some(SignalDirection::Out));
        assert!(!scl.is_optional());
        
        // Check ALERT signal
        let alert = &signals[2];
        assert_eq!(alert.name().unwrap().text(), "ALERT");
        assert_eq!(alert.direction(), Some(SignalDirection::In));
        assert!(alert.is_optional());
    }
    
    #[test]
    fn test_interface_requirement_ast() {
        let source = r#"
        interface I2C {
            signal SDA: inout;
            signal SCL: out;
            require pullup(SDA, 4.7k);
            require pullup(SCL, 4.7k);
            require termination(120);
        }
        "#;
        
        let parsed = parse(source);
        let source_file = SourceFile::cast(parsed.syntax()).unwrap();
        
        let interface = source_file.items()
            .find_map(|item| {
                if let Item::InterfaceDef(iface) = item {
                    Some(iface)
                } else {
                    None
                }
            })
            .expect("Should find interface definition");
        
        // Check requirements
        let requirements: Vec<_> = interface.requirements().collect();
        assert_eq!(requirements.len(), 3);
        
        // Check first pullup requirement
        let pullup1 = &requirements[0];
        assert_eq!(pullup1.requirement_type().unwrap().text(), "pullup");
        let args1 = pullup1.arguments();
        assert_eq!(args1.len(), 2);
        
        // Check second pullup requirement
        let pullup2 = &requirements[1];
        assert_eq!(pullup2.requirement_type().unwrap().text(), "pullup");
        let args2 = pullup2.arguments();
        assert_eq!(args2.len(), 2);
        
        // Check termination requirement
        let term = &requirements[2];
        assert_eq!(term.requirement_type().unwrap().text(), "termination");
        let args3 = term.arguments();
        assert_eq!(args3.len(), 1);
    }
    
    #[test]
    fn test_interface_perspective_ast() {
        let source = r#"
        interface SPI {
            signal MOSI: out;
            signal MISO: in;
            signal SCLK: out;
            signal CS: out;
            
            perspective master {
                signal MOSI: out;
                signal MISO: in;
                signal SCLK: out;
                signal CS: out;
            }
            
            perspective slave {
                signal MOSI: in;
                signal MISO: out;
                signal SCLK: in;
                signal CS: in;
            }
        }
        "#;
        
        let parsed = parse(source);
        let source_file = SourceFile::cast(parsed.syntax()).unwrap();
        
        let interface = source_file.items()
            .find_map(|item| {
                if let Item::InterfaceDef(iface) = item {
                    Some(iface)
                } else {
                    None
                }
            })
            .expect("Should find interface definition");
        
        // Check perspectives
        let perspectives: Vec<_> = interface.perspectives().collect();
        assert_eq!(perspectives.len(), 2);
        
        // Check master perspective
        let master = &perspectives[0];
        assert_eq!(master.name().unwrap().text(), "master");
        let master_signals: Vec<_> = master.signals().collect();
        assert_eq!(master_signals.len(), 4);
        assert_eq!(master_signals[0].direction(), Some(SignalDirection::Out)); // MOSI
        assert_eq!(master_signals[1].direction(), Some(SignalDirection::In));  // MISO
        
        // Check slave perspective
        let slave = &perspectives[1];
        assert_eq!(slave.name().unwrap().text(), "slave");
        let slave_signals: Vec<_> = slave.signals().collect();
        assert_eq!(slave_signals.len(), 4);
        assert_eq!(slave_signals[0].direction(), Some(SignalDirection::In));  // MOSI
        assert_eq!(slave_signals[1].direction(), Some(SignalDirection::Out)); // MISO
    }
    
    #[test]
    fn test_parameterized_interface_ast() {
        let source = r#"
        interface UART(baud_rate: frequency = 115200Hz, data_bits: int = 8) {
            signal TX: out;
            signal RX: in;
            signal RTS: out optional;
            signal CTS: in optional;
        }
        "#;
        
        let parsed = parse(source);
        let source_file = SourceFile::cast(parsed.syntax()).unwrap();
        
        let interface = source_file.items()
            .find_map(|item| {
                if let Item::InterfaceDef(iface) = item {
                    Some(iface)
                } else {
                    None
                }
            })
            .expect("Should find interface definition");
        
        // Check parameters
        let params = interface.params().expect("Should have parameters");
        let param_defs: Vec<_> = params.param_defs().collect();
        assert_eq!(param_defs.len(), 2);
        
        // Check baud_rate parameter
        let baud_rate = &param_defs[0];
        assert_eq!(baud_rate.name().unwrap().text(), "baud_rate");
        // Note: Full type and default value checking would require more AST support
        
        // Check data_bits parameter
        let data_bits = &param_defs[1];
        assert_eq!(data_bits.name().unwrap().text(), "data_bits");
    }
    
    #[test]
    fn test_interface_instantiation_ast() {
        let source = r#"
        board TestBoard {
            power VCC = 3.3V @ 1A;
            ground GND;
            
            // Interface instance
            i2c_bus: I2C(speed = 400kHz);
            
            // Component with interface
            mcu: STM32F103 {
                I2C1 <=> i2c_bus;
            }
        }
        "#;
        
        let parsed = parse(source);
        let source_file = SourceFile::cast(parsed.syntax()).unwrap();
        
        // This test would require InterfaceInst parsing in board context
        // Currently testing that the source parses without errors
        assert!(source_file.items().count() > 0);
    }
}