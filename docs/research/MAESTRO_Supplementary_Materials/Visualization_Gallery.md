# Visualization Gallery

This document contains key visualizations from the MAESTRO evaluation, including convergence plots, circuit diagrams, and performance comparisons.

## 1. Convergence Behavior Visualizations

### 1.1 Series-5-LEDs Progressive Activation

```
Residual vs Iteration (log scale)
1e2  |*
1e0  | *
1e-2 |  **
1e-4 |    ***     Step 1: LED1 only
1e-6 |       ****
1e-8 |           *******
1e-10|                  ********
1e-12|                          *****
     |-------|-------|-------|-------|
     0      50     100    150    200
     
Legend:
Step 1 (0-31): LED1 active
Step 2 (32-80): LED1-2 active  
Step 3 (81-153): LED1-3 active
Step 4 (154-241): LED1-4 active
Step 5 (242-342): All LEDs active
```

### 1.2 Solver Comparison on Series-3-LEDs

```
Residual Evolution Comparison
1e6  |N
1e4  |N    G
1e2  |N    G
1e0  |     G
1e-2 |     G    M
1e-4 |     G    M
1e-6 |          M
1e-8 |     G    M
1e-10|     G    M
1e-12|     G****M***
     |-------|-------|
     0     1000    2000
     
N: Newton-Raphson (diverged)
G: GLACIER (slow convergence)
M: MAESTRO (fast convergence)
```

### 1.3 Current Distribution in Parallel-5-LEDs

```
Current Distribution Over Progressive Steps
50 |■
40 |■■
30 |■■■      Step 1: Strongest LED only
20 |■■■■
10 |■■■■■
0  |-----
   LED1

50 |■■■
40 |■■■■
30 |■■■■■    Step 3: Three LEDs active
20 |■■■■■
10 |■■■■■
0  |-----
   1 2 3

50 |■■■■■
40 |■■■■■
30 |■■■■■    Step 5: All LEDs active
20 |■■■■■    (uneven distribution)
10 |■■■■■
0  |-----
   1 2 3 4 5

Current (mA): LED1=45.2, LED2=32.1, LED3=28.7, LED4=24.3, LED5=18.0
```

## 2. Circuit Topology Visualizations

### 2.1 Series LED Chain with Progressive Activation

```
Step 1: LED1 Active
VCC ──[R:100Ω]──┬──[LED1:ON]──┬──[R:10MΩ]──┬──[R:10MΩ]──┬──[R:10MΩ]──┬── GND
                 │              │            │            │            │
                V=2.78V        V=0.002V     V=0.001V    V=0.001V    V=0V
                I=22.2mA

Step 5: All LEDs Active  
VCC ──[R:100Ω]──┬──[LED1:ON]──┬──[LED2:ON]──┬──[LED3:ON]──┬──[LED4:ON]──┬── GND
                 │              │             │             │             │
                V=4.91V        V=3.11V       V=2.67V      V=0.78V      V=0V
                I=0.92mA
```

### 2.2 Parallel LED Array with Current Sharing

```
        ┌──[LED1: Is=1e-14]── I₁=45.2mA
        │
VCC ──[R:10Ω]──┼──[LED2: Is=3e-14]── I₂=32.1mA
        │
        ├──[LED3: Is=1e-15]── I₃=28.7mA
        │
        ├──[LED4: Is=3e-15]── I₄=24.3mA
        │
        └──[LED5: Is=1e-15]── I₅=18.0mA
                                    ↓
                                   GND
Total current: 148.3mA
Node voltage: 2.95V
```

### 2.3 Buck Converter Progressive Startup

```
Stage 1: Switch Off (Inductor Discharged)
VIN ──[SW:OFF]──┬──[L:10µH]──┬── VOUT
                 │             │
                [D:ON]        [C:100µF]
                 │             │
                GND           GND
                
Stage 2: 5% Duty Cycle
VIN ──[SW:5%]───┬──[L:10µH]──┬── VOUT=0.6V
                 │             │
                [D:95%]       [C:100µF]
                 │             │
                GND           GND

Stage 4: 42% Duty Cycle (Steady State)
VIN ──[SW:42%]──┬──[L:10µH]──┬── VOUT=5.0V
                 │             │
                [D:58%]       [C:100µF]
                 │             │
                GND           GND
```

## 3. Performance Comparison Charts

### 3.1 Convergence Rate by Circuit Category

```
Convergence Rate (%)
100 |                    ████
90  |              ████  ████
80  |        ████  ████  ████
70  |  ████  ████  ████  ████
60  |  ████  ████  ████  ████
50  |  ████  ████  ████  ████
40  |  ████  ████  ████  ████
30  |  ████  ████  ████  ████
20  |  ████  ████  ████  ████
10  |  ████  ████  ████  ████
0   |------------------------
     Series  Para  Power  Amp  Bridge  Prot

Legend: █ Newton  █ GLACIER  █ MAESTRO  █ MAESTRO+G
```

### 3.2 Average Iteration Count (Log Scale)

```
Iterations
10000 |      ████
1000  |      ████
100   |████  ████  ████  ████
10    |████  ████  ████  ████
1     |------------------------
      Newton GLACIER MAESTRO M+G
      
Actual values:
Newton: 127.3
GLACIER: 1,847.2
MAESTRO: 318.7
MAESTRO+G: 287.4
```

### 3.3 Strategy Distribution

```
Strategy Usage in MAESTRO (48 converged cases)

Progressive Activation: ████████████████████ 47.9% (23)
Symmetry Exploitation:  ███████████ 22.9% (11)
Hierarchical Decomp:    ████████ 16.7% (8)
Current Sharing:        ███████ 14.6% (7)
```

## 4. Jacobian Condition Number Evolution

### 4.1 Series-5-LEDs Condition Numbers

```
Condition Number (log scale)
1e16 |                    D
1e14 |                   D
1e12 |                  D
1e10 |                 *
1e8  |               **
1e6  |             ***
1e4  |          ****
1e2  |    ******
1e0  |****
     |-------|-------|-------|
     1      2      3      4      5
     Progressive Activation Step

D: Direct solve attempt (failed)
*: Progressive solve (succeeded)
```

### 4.2 Scaling Impact on Condition Number

```
Before/After Automatic Scaling

Circuit         Before      After       Improvement
Series-3-LEDs   5.6e14     3.2e11      1,750x
Parallel-5      8.9e8      4.5e6       198x
Buck-Basic      2.3e7      1.2e5       192x
Bridge-6Ph      7.8e9      5.6e7       139x
```

## 5. Time Performance Heatmap

### 5.1 Solution Time by Solver and Circuit Type

```
Time (ms)    Newton  GLACIER  MAESTRO  MAESTRO+G
Series       [FAIL]  [723.4]  [19.7]   [18.9]
Parallel     [7.8]   [98.7]   [34.2]   [32.1]
Power        [FAIL]  [534.2]  [45.3]   [42.7]
Amplifier    [FAIL]  [287.3]  [38.7]   [36.2]
Bridge       [FAIL]  [567.8]  [89.3]   [86.7]
Protection   [FAIL]  [FAIL]   [56.7]   [53.2]

Color scale: Green(<50ms) Yellow(50-200ms) Orange(200-500ms) Red(>500ms) Black(FAIL)
```

## 6. Progressive Activation Detailed View

### 6.1 Voltage Evolution in Series-5-LEDs

```
Node Voltages Through Progressive Steps

Step  VCC  N1    N2    N3    N4    GND
1     5.0  2.78  0.00  0.00  0.00  0.0
2     5.0  4.35  2.01  0.00  0.00  0.0
3     5.0  4.73  2.61  0.41  0.00  0.0
4     5.0  4.86  2.74  0.54  0.12  0.0
5     5.0  4.91  3.11  2.67  0.78  0.0

Current Evolution:
Step 1: 22.2mA (LED1 only)
Step 2: 6.5mA (LED1+2)
Step 3: 2.7mA (LED1+2+3)
Step 4: 1.4mA (LED1+2+3+4)
Step 5: 0.92mA (All LEDs)
```

### 6.2 Residual Components Analysis

```
Residual Breakdown by Equation Type

KCL Residuals (Current Conservation):
Step 1: 1e-8 → 1e-12
Step 2: 1e-6 → 1e-12
Step 3: 1e-5 → 1e-13
Step 4: 1e-4 → 1e-13
Step 5: 1e-3 → 1e-13

Component Model Residuals:
LED equations: Dominant contributor
Resistor equations: Always < 1e-14
High-R placeholders: Well-conditioned
```

## 7. Strategy Selection Decision Tree

```
Circuit Analysis
    │
    ├─> Series Components Found?
    │       │
    │       ├─> Nonlinear Count ≥ 2?
    │       │       │
    │       │       └─> Progressive Activation ✓
    │       │
    │       └─> Linear Only
    │               │
    │               └─> Direct Solve
    │
    ├─> Parallel Branches Found?
    │       │
    │       ├─> Identical Components?
    │       │       │
    │       │       └─> Symmetry Exploitation ✓
    │       │
    │       └─> Mismatched Parameters?
    │               │
    │               └─> Current Sharing ✓
    │
    └─> Complex Topology?
            │
            ├─> Weakly Coupled Subcircuits?
            │       │
            │       └─> Hierarchical Decomposition ✓
            │
            └─> No Clear Pattern
                    │
                    └─> Fallback to Core Solver
```

## 8. Statistical Distribution Plots

### 8.1 Iteration Count Distribution

```
MAESTRO Iteration Distribution (48 cases)

Frequency
12 |    ████
10 |    ████
8  |████████████
6  |████████████████
4  |████████████████████
2  |████████████████████████
0  |--------------------------------
   0   100  200  300  400  500  600

Mean: 318.7
Median: 267
Std Dev: 234.5
```

### 8.2 Time Distribution Box Plot

```
Solution Time (ms) - Log Scale

1000 |        ○
     |        |
100  |    ┌───┴───┐     ┌─┴─┐
     |    │   G   │  ┌──┤ M ├──┐
10   | ┌──┤       ├──┤  └───┘  │
     | │  └───────┘  └─────────┘
1    | └N─┘
     |--------------------------------
     Newton  GLACIER  MAESTRO  M+G

○ = Outlier
Box = 25th-75th percentile
Line = Median
```

## 9. Real-Time Convergence Animation Frames

### 9.1 MAESTRO Progressive Solving (5 frames)

```
Frame 1 (t=0ms): Initial State
All components inactive, zero current

Frame 2 (t=5ms): Step 1 Complete
LED1 active, 22.2mA established

Frame 3 (t=12ms): Step 2 Complete
LED1+2 active, current dropped to 6.5mA

Frame 4 (t=25ms): Step 3 Complete
LED1+2+3 active, 2.7mA

Frame 5 (t=45ms): Final Solution
All LEDs active, 0.92mA steady state
```

## 10. Summary Performance Radar Chart

```
Performance Metrics (Normalized 0-1)

         Success Rate
              1.0
           /     \
          /       \
    Speed 0.5   0.0 Robustness
         \         /
          \       /
           \     /
         Efficiency

Newton-Raphson: ▲ (Small triangle)
GLACIER: ■ (Medium square)
MAESTRO: ● (Large circle)
MAESTRO+G: ★ (Fills entire chart)
```

## Appendix: Generating These Visualizations

All visualizations can be regenerated using:

```bash
cd Code_Repository/visualization/
python generate_all_plots.py --data ../Raw_Data/maestro_results.csv
```

Individual plots:
- `plot_convergence.py`: Convergence behavior plots
- `plot_circuits.py`: Circuit topology diagrams
- `plot_performance.py`: Performance comparison charts
- `plot_statistics.py`: Statistical distribution plots

SVG outputs are saved to `outputs/figures/`