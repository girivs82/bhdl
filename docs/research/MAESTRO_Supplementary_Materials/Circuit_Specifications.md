# Detailed Circuit Specifications

This document provides complete specifications for all test circuits, including netlists, component values, and test conditions.

## 1. Series Nonlinear Circuits

### 1.1 Series-N-LEDs Family

**Base Template**:
```spice
* Series-N-LEDs Circuit
.param N=3
V1 VCC 0 DC 5V
R1 VCC N1 {N<=3 ? 100 : 47}
D1 N1 N2 LED1
D2 N2 N3 LED2
...
DN N{N} 0 LEDN

.model LED1 D (IS=1e-30 N=1.7 VJ=0.7 RS=10)
.model LED2 D (IS=1e-35 N=1.8 VJ=0.7 RS=10)
...
```

**Parameter Progression**:
| LED # | Is (A) | n | Vf (V) | Color |
|-------|--------|---|--------|-------|
| 1 | 1e-24 | 1.7 | 1.8 | Red |
| 2 | 1e-28 | 1.8 | 2.0 | Yellow |
| 3 | 1e-32 | 1.8 | 2.2 | Green |
| 4 | 1e-36 | 1.9 | 3.0 | Blue |
| 5 | 1e-38 | 2.0 | 3.2 | White |

### 1.2 Mixed-LED-Diode-5

```spice
* Mixed LED and Diode Series Chain
V1 VCC 0 DC 5V
R1 VCC N1 68
D1 N1 N2 LED_RED
D2 N2 N3 DIODE_1N4148
D3 N3 N4 LED_GREEN
D4 N4 N5 DIODE_1N4148
D5 N5 0 LED_BLUE

.model LED_RED D (IS=1e-30 N=1.7 VJ=0.7 RS=10)
.model LED_GREEN D (IS=1e-32 N=1.8 VJ=0.7 RS=10)
.model LED_BLUE D (IS=1e-35 N=2.0 VJ=0.7 RS=10)
.model DIODE_1N4148 D (IS=2.52e-9 N=1.752 VJ=0.7 RS=0.568)
```

### 1.3 Voltage-Multiplier-N

```spice
* N-Stage Cockroft-Walton Voltage Multiplier
V1 IN 0 AC 12V 60Hz
* Stage 1
C1 IN N1 100nF
D1 0 N1 DIODE
C2 N1 N2 100nF
D2 N2 OUT1 DIODE
* Stage 2 (if N>=2)
C3 IN N3 100nF
D3 N2 N3 DIODE
C4 N3 N4 100nF
D4 N4 OUT2 DIODE
* Continue pattern...

.model DIODE D (IS=1e-12 N=1.0 VJ=0.7 RS=0.1)
```

## 2. Parallel Array Circuits

### 2.1 Parallel-N-LEDs

**Without Ballast Resistors**:
```spice
* Parallel-N-LEDs without ballast
V1 VCC 0 DC 5V
R_MAIN VCC COMMON 10
D1 COMMON 0 LED_1
D2 COMMON 0 LED_2
...
DN COMMON 0 LED_N

* Slight parameter variations (±20%)
.model LED_1 D (IS=1.0e-15 N=1.8 VJ=0.7 RS=10)
.model LED_2 D (IS=1.1e-15 N=1.8 VJ=0.7 RS=10)
.model LED_3 D (IS=0.9e-15 N=1.8 VJ=0.7 RS=10)
```

**With Ballast Resistors**:
```spice
* Parallel-N-LEDs with ballast
V1 VCC 0 DC 5V
R_MAIN VCC COMMON 10

* Branch 1
R_B1 COMMON LED1_A 1
D1 LED1_A 0 LED

* Branch 2
R_B2 COMMON LED2_A 1
D2 LED2_A 0 LED

* Continue pattern...
.model LED D (IS=1e-15 N=1.8 VJ=0.7 RS=10)
```

### 2.2 Parallel-Mismatched-5

```spice
* 5 Parallel LEDs with 10x Is variation
V1 VCC 0 DC 5V
R_MAIN VCC COMMON 10

D1 COMMON 0 LED_STRONG1
D2 COMMON 0 LED_STRONG2  
D3 COMMON 0 LED_MEDIUM
D4 COMMON 0 LED_WEAK1
D5 COMMON 0 LED_WEAK2

.model LED_STRONG1 D (IS=1e-14 N=1.8)

.model LED_STRONG2 D (IS=3e-14 N=1.8)

.model LED_MEDIUM D (IS=1e-15 N=1.8)

.model LED_WEAK1 D (IS=3e-15 N=1.8)

.model LED_WEAK2 D (IS=1e-15 N=1.8)
```

## 3. Power Converter Circuits

### 3.1 Buck-Basic

```spice
* Basic Buck Converter
V1 VIN 0 DC 12V
* Switch (simplified as voltage-controlled switch)
S1 VIN SW CTRL 0 SWITCH
D1 0 SW SCHOTTKY
L1 SW VOUT 10uH
C1 VOUT 0 100uF ESR=0.02
R_LOAD VOUT 0 10

* Control voltage (duty cycle)
V_CTRL CTRL 0 PULSE(0 1 0 10n 10n {DUTY*T} {T})
.param DUTY=0.42
.param T=10u

.model SWITCH SW (RON=0.01 ROFF=10MEG)
.model SCHOTTKY D (IS=1e-9 N=1.05 RS=0.05)
```

### 3.2 Buck-SoftStart

```spice
* Buck with Soft-Start
* Same as Buck-Basic but with:
V_SS SS_OUT 0 PWL(0 0 100u 0.42)
E_CTRL CTRL 0 SS_OUT 0 1
* This ramps duty cycle from 0 to 42% over 100us
```

### 3.3 Boost-Basic

```spice
* Basic Boost Converter
V1 VIN 0 DC 5V
L1 VIN SW 10uH
* Switch to ground
S1 SW 0 CTRL 0 SWITCH
D1 SW VOUT SCHOTTKY
C1 VOUT 0 100uF ESR=0.02
R_LOAD VOUT 0 100

V_CTRL CTRL 0 PULSE(0 1 0 10n 10n {DUTY*T} {T})
.param DUTY=0.6
.param T=10u
```

### 3.4 SEPIC Converter

```spice
* SEPIC Converter
V1 VIN 0 DC 12V
L1 VIN N1 10uH
C1 N1 N2 10uF
S1 N1 0 CTRL 0 SWITCH
L2 N2 0 10uH
D1 N2 VOUT SCHOTTKY
C2 VOUT 0 100uF
R_LOAD VOUT 0 50

V_CTRL CTRL 0 PULSE(0 1 0 10n 10n 6u 10u)
```

## 4. Cascaded Amplifier Circuits

### 4.1 Cascade-N-Stage

```spice
* N-Stage Cascaded Amplifier
V1 VCC 0 DC 12V
V_IN INPUT 0 AC 0.1V 1kHz

* Stage 1
R_IN1 INPUT 0 10k
E1 STAGE1 0 INPUT 0 10
R_OUT1 VCC STAGE1 1k

* Stage 2  
C_COUP1 STAGE1 IN2 10uF
R_IN2 IN2 0 10k
E2 STAGE2 0 IN2 0 20
R_OUT2 VCC STAGE2 1k

* Stage 3
C_COUP2 STAGE2 IN3 10uF
R_IN3 IN3 0 10k
E3 OUTPUT 0 IN3 0 15
R_OUT3 VCC OUTPUT 1k

* Gains: 10 * 20 * 15 = 3000 (69.5dB)
```

### 4.2 Cascade-Feedback

```spice
* 2-Stage with Negative Feedback
V1 VCC 0 DC 12V
V_IN INPUT 0 AC 0.1V

* Stage 1
R_IN INPUT N1 1k
E1 STAGE1 0 N1 0 100
R_OUT1 VCC STAGE1 1k

* Stage 2
E2 OUTPUT 0 STAGE1 0 50

* Feedback network
R_FB OUTPUT N1 99k
R_FB_GND N1 0 1k
* Closed loop gain = 100

C_COMP N1 0 10pF
* Compensation for stability
```

## 5. Bridge Circuits

### 5.1 Bridge-Rectifier-Basic

```spice
* Full-Wave Bridge Rectifier
V1 AC1 AC2 SIN(0 14.14 60)
D1 AC1 DC_POS DIODE
D2 DC_NEG AC1 DIODE  
D3 AC2 DC_POS DIODE
D4 DC_NEG AC2 DIODE
C1 DC_POS DC_NEG 1000uF ESR=0.1
R_LOAD DC_POS DC_NEG 100

.model DIODE D (IS=1e-12 N=1.0 RS=0.1)
```

### 5.2 Bridge-3-Phase

```spice
* 3-Phase Bridge Rectifier
V_A PH_A 0 SIN(0 170 60 0)
V_B PH_B 0 SIN(0 170 60 0 0 -120)
V_C PH_C 0 SIN(0 170 60 0 0 -240)

* Positive group
D1 PH_A DC_POS DIODE
D3 PH_B DC_POS DIODE
D5 PH_C DC_POS DIODE

* Negative group
D2 DC_NEG PH_A DIODE
D4 DC_NEG PH_B DIODE
D6 DC_NEG PH_C DIODE

C1 DC_POS DC_NEG 1000uF
R_LOAD DC_POS DC_NEG 100
```

### 5.3 Bridge-Active-PFC

```spice
* Simplified Active PFC Bridge
V_AC AC1 AC2 SIN(0 170 60)
* Input bridge
D1 AC1 RECT_POS DIODE
D2 RECT_NEG AC1 DIODE
D3 AC2 RECT_POS DIODE
D4 RECT_NEG AC2 DIODE

* Boost PFC stage
L_BOOST RECT_POS SW_NODE 1mH
S_BOOST SW_NODE RECT_NEG CTRL 0 MOSFET
D_BOOST SW_NODE DC_OUT DIODE
C_BULK DC_OUT RECT_NEG 470uF
R_LOAD DC_OUT RECT_NEG 200

* PFC controller (simplified)
V_CTRL CTRL 0 PULSE(0 1 0 1n 1n 8u 10u)
```

## 6. Protection Circuits

### 6.1 Protection-OVP-TVS

```spice
* Overvoltage Protection with TVS
V1 VIN 0 DC 5V
R_SERIES VIN PROTECTED 10
D_TVS PROTECTED 0 TVS_6V
R_LOAD PROTECTED 0 1000

.model TVS_6V D (
+ IS=1e-14 N=1.0
+ BV=6.0 IBV=1mA
+ RS=0.1 VJ=0.7
)
* Breakdown at 6V, clamps at ~7.5V
```

### 6.2 Protection-Current-Limit

```spice
* Current Limiter with Foldback
V1 VIN 0 DC 12V
* Current sense resistor
R_SENSE VIN N1 0.1

* Pass transistor (simplified)
G_PASS N1 VOUT CTRL 0 1
R_LOAD VOUT 0 {RLOAD}

* Control circuit
E_SENSE V_SENSE 0 VIN N1 1
* Foldback characteristic
E_CTRL CTRL 0 TABLE {V_SENSE} (0,1 0.5,1 0.6,0.5 1.0,0.1)

.step param RLOAD 1 100 10
```

### 6.3 Protection-Crowbar

```spice
* Crowbar Protection Circuit
V1 VIN 0 DC 12V
F1 VIN FUSED 5A
R_SERIES FUSED PROTECTED 0.1

* Trigger circuit
R_SENSE PROTECTED N1 10k
D_ZENER 0 N1 ZENER_13V5
R_GATE N1 GATE 100

* SCR (simplified as voltage-controlled switch)
S_SCR PROTECTED 0 GATE 0 SCR_MODEL

* Load
R_LOAD PROTECTED 0 100

.model ZENER_13V5 D (BV=13.5 IBV=5mA)
.model SCR_MODEL SW (VT=0.7 RON=0.01 ROFF=10MEG)
```

## Test Conditions

### Environmental Parameters
- Temperature: 25°C (298.15K) for all tests
- No parameter variation unless specified
- Initial conditions: All capacitors discharged, inductors at zero current

### Convergence Criteria
- Absolute tolerance: 1e-12 A for currents
- Relative tolerance: 1e-12 for voltages
- Maximum iterations: 50 (Newton), 10,000 (GLACIER), unlimited (MAESTRO)

### Measurement Points
- Steady-state DC operating point
- All branch currents recorded
- All node voltages recorded
- Power dissipation calculated

### Progressive Activation Settings
- High resistance for "off" components: 10MΩ
- Ramp resolution: 5% increments
- Initial guess propagation: Enabled
- Sub-problem max iterations: 100