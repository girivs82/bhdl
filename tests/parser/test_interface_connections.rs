//! Test parsing of pin-to-interface connections

use bhdl_parser::{parse, ParseResult};

#[test]
fn test_pin_to_interface_connection() {
    let source = r#"
    interface I2C {
        signal SDA: inout;
        signal SCL: out;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Interface instance
        i2c_bus: I2C();
        
        // Component with pin-to-interface connections
        mcu: STM32F4() {
            PA4 -> i2c_bus.SDA;
            PA5 -> i2c_bus.SCL;
        }
    }
    "#;
    
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
}

#[test]
fn test_multiple_pin_to_interface_connections() {
    let source = r#"
    interface SPI {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS: out optional;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        spi1: SPI();
        
        mcu: MCU() {
            // Multiple pins to same interface
            PB3 -> spi1.MOSI;
            PB4 -> spi1.MISO;
            PB5 -> spi1.SCK;
            PB6 -> spi1.CS;
        }
        
        sensor: Sensor() {
            // Another component connecting to same interface
            MOSI <- spi1.MOSI;
            MISO -> spi1.MISO;
            SCK <- spi1.SCK;
            CS <- spi1.CS;
        }
    }
    "#;
    
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
}

#[test]
fn test_interface_signal_in_flow_connection() {
    let source = r#"
    interface UART {
        signal TX: out;
        signal RX: in;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        uart1: UART();
        
        // Direct flow connections with interface signals
        VCC -> Res(10k).1 -> uart1.TX;
        uart1.RX -> Cap(100nF).1 -> GND;
    }
    "#;
    
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
}

#[test]
fn test_interface_to_interface_connection() {
    let source = r#"
    interface Serial {
        signal TX: out;
        signal RX: in;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        uart_debug: Serial();
        uart_comm: Serial();
        
        // Cross-connect two interfaces
        uart_debug.TX -> uart_comm.RX;
        uart_comm.TX -> uart_debug.RX;
    }
    "#;
    
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
}