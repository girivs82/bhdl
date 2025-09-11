# Body Control Module (BCM) - Functional Architecture
## Real Automotive ECU Project

### Project Overview
**Product**: Body Control Module for Mid-Size Vehicle
**Production Target**: 500,000 units/year
**Safety Level**: Mixed (QM to ASIL B)
**Timeline**: 18 months to Start of Production (SOP)

### Functional Block Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                     BODY CONTROL MODULE (BCM)                       │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌──────────────────────┐         ┌──────────────────────────┐     │
│  │   POWER MANAGEMENT    │         │   CENTRAL PROCESSOR      │     │
│  ├──────────────────────┤         ├──────────────────────────┤     │
│  │ • Battery monitoring  │         │ • Main MCU (S32K144)     │     │
│  │ • Sleep/wake control  │◄────────┤ • 512KB Flash            │     │
│  │ • Load management     │         │ • CAN/LIN gateway        │     │
│  │ • Power distribution  │         │ • Diagnostics (UDS)      │     │
│  └──────────────────────┘         └──────────────────────────┘     │
│             ▲                                │                      │
│             │                                │                      │
│  ┌──────────────────────┐         ┌──────────────────────────┐     │
│  │   EXTERIOR LIGHTING  │         │   INTERIOR LIGHTING      │     │
│  ├──────────────────────┤         ├──────────────────────────┤     │
│  │ • Headlights (LED)   │◄────────┤ • Dome lights            │     │
│  │ • Turn signals       │         │ • Reading lights         │     │
│  │ • Brake lights       │         │ • Ambient lighting       │     │
│  │ • DRL control        │         │ • Footwell illumination  │     │
│  └──────────────────────┘         └──────────────────────────┘     │
│                                              │                      │
│  ┌──────────────────────┐         ┌──────────────────────────┐     │
│  │   DOOR CONTROL       │         │   COMFORT FEATURES       │     │
│  ├──────────────────────┤         ├──────────────────────────┤     │
│  │ • Central locking    │◄────────┤ • Wiper control          │     │
│  │ • Window control     │         │ • Washer pump            │     │
│  │ • Mirror adjustment  │         │ • Seat heating relay     │     │
│  │ • Deadlock function  │         │ • Horn control           │     │
│  └──────────────────────┘         └──────────────────────────┘     │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │                    COMMUNICATION                           │     │
│  ├──────────────────────────────────────────────────────────┤     │
│  │ • CAN-FD (Powertrain, Chassis)                            │     │
│  │ • LIN (Doors, Mirrors, Seats)                             │     │
│  │ • Diagnostics (ISO 14229)                                 │     │
│  └──────────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────┘
```

### Functional Requirements

#### Power Management
- **Input**: 12V vehicle battery (9V-16V operating range)
- **Quiescent Current**: <3mA in sleep mode
- **Wake Sources**: CAN activity, door handle, key fob
- **Load Shedding**: Prioritized shutdown under low voltage

#### Exterior Lighting (ASIL B)
- **Headlights**: PWM controlled LED drivers, 2x35W
- **Turn Signals**: Flash rate 90±30 cycles/min
- **Brake Lights**: <300ms activation time (safety critical)
- **DRL**: Automatic with ignition

#### Door Control (ASIL A)
- **Central Locking**: All doors within 2 seconds
- **Window Control**: Anti-trap protection required
- **Auto-up/down**: One-touch operation
- **Child Lock**: Rear door disable function

#### Interior Lighting
- **Dome Light**: Fade in/out, timeout after 30s
- **Reading Lights**: Individual control per seat
- **Ambient**: RGB LED control, multiple zones

#### Communication
- **CAN-FD**: 2 channels @ 2Mbps data rate
- **LIN**: 4 channels @ 19.2kbps
- **Gateway**: CAN to LIN message routing
- **Diagnostics**: UDS over CAN

### Safety-Relevant Functions

| Function | ASIL Level | Safety Goal |
|----------|------------|-------------|
| Brake Light Control | ASIL B | Shall illuminate within 300ms |
| Turn Signal | ASIL A | Shall maintain correct flash rate |
| Window Anti-trap | ASIL B | Shall reverse on obstruction |
| Central Locking | ASIL A | Shall not lock with key inside |
| Headlight Control | QM | Degraded mode if failure |
| Sleep/Wake | QM | Must wake on safety events |

### System Constraints

#### Environmental
- Operating Temperature: -40°C to +85°C
- Storage Temperature: -40°C to +125°C
- Humidity: 95% RH max
- Vibration: ISO 16750-3
- EMC: CISPR 25 Class 3

#### Electrical
- Supply Voltage: 9V to 16V continuous, 6V to 18V transient
- Load Dump: Survive 40V for 400ms
- Reverse Polarity: Protected to -14V
- ESD: ±8kV contact, ±15kV air

#### Mechanical
- PCB Size: 180mm x 140mm max
- Connector: 2x 64-pin automotive grade
- Mounting: 4 points, vibration isolated
- Cooling: Passive (no fan)

### Power Budget Estimation

| Subsystem | Typical Current | Max Current |
|-----------|-----------------|-------------|
| MCU + Memory | 80mA | 150mA |
| CAN Transceivers | 60mA | 100mA |
| LIN Transceivers | 20mA | 40mA |
| Control Circuits | 50mA | 100mA |
| **Total Internal** | **210mA** | **390mA** |
| **External Loads** | **5A typical** | **25A max** |

### Next Steps
1. Safety analysis and requirement allocation
2. Detailed hardware architecture
3. Power supply design
4. Communication interface design
5. Load driver design
6. Protection circuits