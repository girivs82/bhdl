# Basic Port Mapping - The Fundamental Feature

## The Core Problem

When instantiating a module, you need to connect its pins to signals in the parent module. This is the most basic and essential form of port mapping!

## Updated BHDL Syntax (Consistent Left-Right Convention)

### 1. Simple Port Mapping During Instantiation

```bhdl
entity Regulator {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: digital in;
    pin FB: analog in;
    
    // Entity implementation...
}

board PowerSupply {
    power INPUT_12V = 12V;
    power OUTPUT_5V = 5V;
    
    // Basic port mapping - entity pins on LEFT, parent signals on RIGHT
    reg: Regulator {
        VIN <- INPUT_12V;       // Entity input receives from board power
        VOUT -> OUTPUT_5V;      // Entity output sends to board power
        EN <- enable_signal;    // Entity input receives from net
        FB <- feedback_net;     // Entity input receives from net
    }
}
```

### 2. Instance-to-Instance Connections

```bhdl
board System {
    // First entity
    buck: BuckConverter {
        VIN <- VIN_24V;
        VOUT -> intermediate_12v;
    }
    
    // Second entity - connected to first
    ldo: LinearRegulator {
        VIN <- intermediate_12v;  // Connect between modules
        VOUT -> final_5v;
    }
}
```

### 3. Pin Name Mapping in Connection Body

```bhdl
entity PowerStage {
    pin VCC: power in;
    pin OUT: power out;
    pin CONTROL: signal in;
    
    controller: PWMController {
        POWER <- VCC;           // Different pin names!
        PWM_OUT -> driver.IN;   // Internal connection
        FB_IN <- feedback;      // Local signal
    }
    
    driver: GateDriver {
        VDD <- VCC;            // Another name difference
        GATE -> mosfet.G;
        SOURCE -> OUT;
    }
}
```

### 4. Array Pin Mapping

```bhdl
entity LED_Bank {
    pin ANODES[8]: current out;
    pin CATHODES[8]: current in;
    
    // Internal implementation
}

board Display {
    // Array mapping
    bank: LED_Bank {
        ANODES[0..7] <- LED_POWER[0..7];     // Array to array
        CATHODES[0..7] -> DRIVERS[0..7];
    }
}
```

### 5. Selective Pin Mapping

```bhdl
entity FlexibleModule {
    pin REQUIRED: power in;
    pin OPTIONAL_1: signal in;
    pin OPTIONAL_2: signal in;
    pin OUT: signal out;
}

board MinimalUsage {
    flex: FlexibleModule {
        REQUIRED <- VCC;
        OUT -> result;
        // OPTIONAL_1 and OPTIONAL_2 left unconnected
    }
}
```

## What We've Been Calling "Port Mapping"

This is just the basic connection syntax inside entity instantiation blocks:

```bhdl
instance_name: ModuleType {
    // This whole block is "port mapping"!
    input_pin <- source;         // Input mapping
    output_pin -> destination;   // Output mapping
    bidir_pin <-> signal;       // Bidirectional mapping
}
```

## The Syntax Rules

### 1. Input Pins
```bhdl
entity_input_pin <- external_signal;
```

### 2. Output Pins  
```bhdl
entity_output_pin -> external_signal;
```

### 3. Bidirectional Pins
```bhdl
entity_bidir_pin <-> external_signal;
```

### 4. Direct Instance-to-Instance
```bhdl
entity2_input <- entity1.output_pin;
```

### 5. Through Intermediate Signals
```bhdl
entity Container {
    signal intermediate_net;
    
    source: SourceModule {
        OUT -> intermediate_net;
    }
    
    sink: SinkModule {
        IN <- intermediate_net;
    }
}
```

## Complete Example

```bhdl
entity UARTTransceiver {
    pin TX: signal out;
    pin RX: signal in;
    pin CTS: signal in;
    pin RTS: signal out;
    pin VCC: power in;
    pin GND: ground;
}

entity LevelShifter {
    pin A: signal inout;
    pin B: signal inout;
    pin VCCA: power in;
    pin VCCB: power in;
}

board SerialInterface {
    power VCC_3V3 = 3.3V;
    power VCC_5V = 5V;
    ground GND;
    
    // MCU UART at 3.3V
    mcu_uart: UARTTransceiver {
        VCC <- VCC_3V3;         // Power mapping
        GND <- GND;             // Ground mapping
        TX -> uart_tx_3v3;      // Output signal mapping
        RX <- uart_rx_3v3;      // Input signal mapping
        CTS <- uart_cts_3v3;    // Flow control mapping
        RTS -> uart_rts_3v3;
    }
    
    // Level shifters for 3.3V <-> 5V
    tx_shifter: LevelShifter {
        A <-> uart_tx_3v3;      // 3.3V side
        B <-> uart_tx_5v;       // 5V side
        VCCA <- VCC_3V3;
        VCCB <- VCC_5V;
    }
    
    rx_shifter: LevelShifter {
        A <-> uart_rx_3v3;
        B <-> uart_rx_5v;
        VCCA <- VCC_3V3;
        VCCB <- VCC_5V;
    }
    
    // External UART at 5V
    external_uart: UARTTransceiver {
        VCC <- VCC_5V;
        GND <- GND;
        RX <- uart_tx_5v;       // Note TX->RX swap!
        TX -> uart_rx_5v;       // And RX->TX swap!
        // CTS/RTS not connected in this example
    }
    
    // External connector
    connector: DB9 {
        PIN_3 <- external_uart.TX;   // Direct module-to-module
        PIN_2 -> external_uart.RX;   // Through module
        PIN_5 <- GND;
    }
}
```

## Why This is Fundamental

1. **It's How You Wire Entities** - Without port mapping, entities can't connect to anything!

2. **Name Independence** - Entity pins can have generic names; mapping provides context:
   ```bhdl
   reg1: Regulator {
       VIN <- V12V;
       VOUT -> V5V;
   }
   
   reg2: Regulator {
       VIN <- V12V;  
       VOUT -> V3V3;  // Same entity, different usage
   }
   ```

3. **Hierarchy Support** - Signals at board level map to entity pins, which map to sub-entity pins:
   ```bhdl
   board -> entity -> subentity -> component
   ```

4. **Type Checking** - The analyzer verifies signal types match:
   ```bhdl
   power_in <- power_rail;   // OK
   power_in <- digital_signal; // ERROR: type mismatch
   ```

5. **Consistent Syntax** - Entity pins always on left, signals on right:
   - Easy to scan vertically to see all entity pins
   - Arrow direction shows actual data flow
   - No ambiguity about what's being connected