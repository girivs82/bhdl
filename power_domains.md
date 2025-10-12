# Power Domain Documentation

**Generated**: 2025-10-12 13:02:08

---

## Voltage Domain Summary

| Domain | Connections | Components | Decoupling | Total Capacitance |
|--------|-------------|------------|------------|-------------------|
| @VCC_3V3 | 5 | 3 | 15 caps | 21.8 µF |
| **Total** | **5** | **3** | **15 caps** | - |

### Statistics

- **Power Domains**: 1
- **Total Connections**: 5
- **Unique Components**: 3
- **Decoupling Capacitors**: 15


---

## Power Tree

```
Power Distribution
  ├─ VCC_3V3 → 5 components
```



---

## Power Budget Analysis

### Domain-Level Budgets

| Domain | Total Current | Component Count | Peak Current | Margin | Status |
|--------|---------------|-----------------|--------------|--------|--------|
| @VCC_3V3 | 0.0 mA | 5 | 0.0 mA | 30% | ⚠ Adequate |

### Overall Summary

- **Total Power Consumption**: 0.00 W
- **Power Domains**: 1
- **Total Components**: 5

### Detailed Breakdown

#### @VCC_3V3

**Notes**:

- Peak current estimated as 1.5× typical
- 5 components missing current specifications

---



---

## Bill of Materials

### Decoupling Capacitors

| Ref Des | Value | Quantity | Type | Voltage | Placement |
|---------|-------|----------|------|---------|-----------|\n| C1-C4 | 100nF | 4 | Ceramic | 25V | Near-component |
| C1-C8 | 47nF | 8 | Ceramic | 25V | Distributed |
| C1-C2 | 10µF | 2 | Ceramic | 16V | Near-component |
| C1 | 1µF | 1 | Ceramic | 16V | Near-component |

**Summary**: 15 capacitors total, 4 unique values



---

## Power Domain Connections

### @VCC_3V3

**Connections** (5 total):

*Pattern Expansion:*
- r1.* (2 pins): 2 connections
- r2.* (2 pins): 2 connections

- VCC_3V3 → r1.r1
- VCC_3V3 → r2.r2
- VCC_3V3 → r1.r
- VCC_3V3 → r2.r
- VCC_3V3 → r3.r

**Decoupling** (15 capacitors):

*Near-component placement:*
- 4× 100nF
- 2× 10µF
- 1× 1µF

*Distributed placement:*
- 8× 47nF

---



