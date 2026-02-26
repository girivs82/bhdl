#!/usr/bin/env node
// ascii_schematic.mjs — Text-grid renderer for BHDL schematic layouts
//
// Renders the computed layout as an ASCII character grid that can be
// read by humans or by Claude to understand spatial arrangement.
//
// Usage:
//   node ascii_schematic.mjs <schematic-data.json>
//   node ascii_schematic.mjs <schematic-data.json> --compact
//   node ascii_schematic.mjs --help

import { computeLayout } from './layout_engine.mjs';
import { readFileSync } from 'fs';

// ═══════════════════ CLI ═══════════════════

const args = process.argv.slice(2);
if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
    console.error(`Usage: node ascii_schematic.mjs <schematic-data.json> [options]

Renders a SchematicData JSON file as an ASCII text grid.

Options:
  --compact     Use tighter character scaling (6px/char instead of 8)
  --wide        Use wider character scaling (10px/char)
  --no-wires    Don't draw wire segments
  --help, -h    Show this help
`);
    process.exit(args.includes('--help') || args.includes('-h') ? 0 : 1);
}

const compact = args.includes('--compact');
const wide = args.includes('--wide');
const noWires = args.includes('--no-wires');
const jsonPath = args.find(a => !a.startsWith('-'));

if (!jsonPath) {
    console.error('Error: no input JSON file specified');
    process.exit(1);
}

// ═══════════════════ LOAD & COMPUTE ═══════════════════

let data;
try {
    data = JSON.parse(readFileSync(jsonPath, 'utf8'));
} catch (e) {
    console.error(`Error reading ${jsonPath}: ${e.message}`);
    process.exit(1);
}

let layout;
try {
    layout = computeLayout(data);
} catch (e) {
    console.error(`Layout computation failed: ${e.message}`);
    process.exit(1);
}

// ═══════════════════ GRID SETUP ═══════════════════

// Scale factors: how many pixels per character cell
const SCALE_X = compact ? 6 : wide ? 10 : 8;
const SCALE_Y = compact ? 12 : wide ? 20 : 16;

// Grid: Map<"col,row", char>
const grid = new Map();
// Layer priority: higher number overwrites lower
const gridLayer = new Map();

function setCell(col, row, ch, layer = 0) {
    const key = `${col},${row}`;
    const existing = gridLayer.get(key) || -1;
    if (layer >= existing) {
        grid.set(key, ch);
        gridLayer.set(key, layer);
    }
}

function getCell(col, row) {
    return grid.get(`${col},${row}`) || ' ';
}

// ═══════════════════ DRAWING PRIMITIVES ═══════════════════

function drawBox(x, y, w, h, layer = 1) {
    // Top and bottom edges
    for (let c = x + 1; c < x + w - 1; c++) {
        setCell(c, y, '─', layer);
        setCell(c, y + h - 1, '─', layer);
    }
    // Left and right edges
    for (let r = y + 1; r < y + h - 1; r++) {
        setCell(x, r, '│', layer);
        setCell(x + w - 1, r, '│', layer);
    }
    // Corners
    setCell(x, y, '┌', layer);
    setCell(x + w - 1, y, '┐', layer);
    setCell(x, y + h - 1, '└', layer);
    setCell(x + w - 1, y + h - 1, '┘', layer);
}

function writeText(x, y, text, layer = 2) {
    for (let i = 0; i < text.length; i++) {
        setCell(x + i, y, text[i], layer);
    }
}

function drawHLine(x1, x2, y, ch = '─', layer = 0) {
    const start = Math.min(x1, x2);
    const end = Math.max(x1, x2);
    for (let c = start; c <= end; c++) {
        const existing = getCell(c, y);
        if (existing === '│') {
            setCell(c, y, '┼', layer);
        } else if (existing === '┌' || existing === '└' || existing === '├') {
            // Don't overwrite box corners
        } else {
            setCell(c, y, ch, layer);
        }
    }
}

function drawVLine(x, y1, y2, ch = '│', layer = 0) {
    const start = Math.min(y1, y2);
    const end = Math.max(y1, y2);
    for (let r = start; r <= end; r++) {
        const existing = getCell(x, r);
        if (existing === '─') {
            setCell(x, r, '┼', layer);
        } else if (existing === '┌' || existing === '┐' || existing === '┬') {
            // Don't overwrite box corners
        } else {
            setCell(x, r, ch, layer);
        }
    }
}

// ═══════════════════ RENDER ELEMENTS ═══════════════════

// Find global bounding box to compute offset
let minPx = Infinity, minPy = Infinity;
for (const el of layout.elements) {
    if (el.x != null) minPx = Math.min(minPx, el.x);
    if (el.y != null) minPy = Math.min(minPy, el.y);
}
// Add padding
const PAD_COLS = 2;
const PAD_ROWS = 1;
const offsetX = -(minPx || 0) + PAD_COLS * SCALE_X;
const offsetY = -(minPy || 0) + PAD_ROWS * SCALE_Y;

function toCol(px) { return Math.round((px + offsetX) / SCALE_X); }
function toRow(py) { return Math.round((py + offsetY) / SCALE_Y); }

// Draw components
for (const el of layout.elements) {
    if (el.x == null || el.y == null || el.w == null || el.h == null) continue;

    const col = toCol(el.x);
    const row = toRow(el.y);
    const w = Math.max(3, Math.round(el.w / SCALE_X));
    const h = Math.max(3, Math.round(el.h / SCALE_Y));

    if (el.type === 'power_source') {
        // Power sources: draw as a flag/label
        const label = el.label || el.name.replace(/__pwr_/, '').replace(/__$/, '');
        const shortLabel = label.length > 12 ? label.substring(0, 12) : label;
        writeText(col, row, `◄${shortLabel}►`, 3);
        // Draw output wire stub
        const outCol = col + shortLabel.length + 2;
        setCell(outCol, row, '─', 0);
        setCell(outCol + 1, row, '─', 0);
        continue;
    }

    if (el.type === 'entity_in' || el.type === 'entity_out') {
        drawBox(col, row, w, h, 2);
        const nameStr = (el.name || 'entity').substring(0, w - 2);
        writeText(col + 1, row + 1, nameStr, 3);
        continue;
    }

    // Instance components
    const displayName = el.displayName || el.name;

    if (el.category && ['resistor', 'capacitor', 'inductor', 'diode', 'protection'].includes(el.category)) {
        // Symbol components: draw as small inline symbols
        if (el.isShunt) {
            // Vertical symbol
            const cx = toCol(el.x + el.w / 2);
            const cy = toRow(el.y + el.h / 2);
            const sym = el.category === 'resistor' ? '╫' :
                        el.category === 'capacitor' ? '╪' :
                        el.category === 'inductor' ? '⌇' :
                        el.category === 'diode' ? '▽' : '◇';
            setCell(cx, cy, sym, 2);
            // Label to the left
            const label = displayName.substring(0, 8);
            writeText(cx - label.length - 1, cy, label, 1);
            // Value to the right
            const value = (el.parameters || []).find(p => p[0] === 'value');
            if (value && value[1]) {
                writeText(cx + 2, cy, value[1].substring(0, 6), 1);
            }
        } else {
            // Horizontal symbol
            const cx = toCol(el.x + el.w / 2);
            const cy = toRow(el.y + el.h / 2);
            const sym = el.category === 'resistor' ? '┤├' :
                        el.category === 'capacitor' ? '│├' :
                        el.category === 'inductor' ? '⌇⌇' :
                        el.category === 'diode' ? '▷│' : '◇│';
            writeText(cx - 1, cy, sym, 2);
            // Label above
            const label = displayName.substring(0, 10);
            writeText(cx - Math.floor(label.length / 2), cy - 1, label, 1);
            // Value below
            const value = (el.parameters || []).find(p => p[0] === 'value');
            if (value && value[1]) {
                const valStr = value[1].substring(0, 8);
                writeText(cx - Math.floor(valStr.length / 2), cy + 1, valStr, 1);
            }
        }
    } else {
        // IC / regulator / generic box component
        drawBox(col, row, w, h, 1);

        // Name centered in top row
        const nameStr = displayName.substring(0, w - 2);
        writeText(col + Math.max(1, Math.floor((w - nameStr.length) / 2)), row + 1, nameStr, 3);

        // Entity type below name
        if (el.entityType && h > 3) {
            const typeStr = el.entityType.substring(0, w - 2);
            writeText(col + Math.max(1, Math.floor((w - typeStr.length) / 2)), row + 2, typeStr, 2);
        }

        // Port labels
        for (const p of (el.inputPorts || [])) {
            const pr = toRow(p.y);
            const label = p.name.substring(0, 4);
            writeText(col + 1, pr, label, 2);
            // Input stub
            setCell(col - 1, pr, '─', 0);
        }
        for (const p of (el.outputPorts || [])) {
            const pr = toRow(p.y);
            const label = p.name.substring(0, 4);
            writeText(col + w - label.length - 1, pr, label, 2);
            // Output stub
            setCell(col + w, pr, '─', 0);
        }
    }
}

// Draw GND symbols
for (const el of layout.elements) {
    if (!el.gndStubs || el.gndStubs.length === 0) continue;
    if (el.x == null || el.y == null) continue;

    const cx = toCol(el.x + el.w / 2);
    const bottomRow = toRow(el.y + el.h);

    if (el.isShunt) {
        // GND below shunt component
        const gndRow = bottomRow + 1;
        setCell(cx, gndRow, '▼', 2);
        writeText(cx - 1, gndRow + 1, 'GND', 1);
    } else {
        // GND stub on side
        for (const stub of el.gndStubs) {
            setCell(cx, bottomRow + 1, '▼', 2);
        }
    }
}

// Draw wires
if (!noWires) {
    for (const wire of layout.wires) {
        for (const seg of (wire.segments || [])) {
            const c1 = toCol(seg.x1), r1 = toRow(seg.y1);
            const c2 = toCol(seg.x2), r2 = toRow(seg.y2);

            if (Math.abs(r1 - r2) < 1) {
                // Horizontal
                const ch = wire.isPower ? '═' : '─';
                drawHLine(c1, c2, r1, ch, 0);
            } else if (Math.abs(c1 - c2) < 1) {
                // Vertical
                const ch = wire.isPower ? '║' : '│';
                drawVLine(c1, r1, r2, ch, 0);
            }
        }
    }
}

// ═══════════════════ RENDER GRID TO STRING ═══════════════════

let maxCol = 0, maxRow = 0;
for (const key of grid.keys()) {
    const [c, r] = key.split(',').map(Number);
    maxCol = Math.max(maxCol, c);
    maxRow = Math.max(maxRow, r);
}

const lines = [];
for (let r = 0; r <= maxRow; r++) {
    let line = '';
    let lastNonSpace = 0;
    for (let c = 0; c <= maxCol; c++) {
        const ch = getCell(c, r);
        line += ch;
        if (ch !== ' ') lastNonSpace = line.length;
    }
    lines.push(line.substring(0, lastNonSpace)); // trim trailing spaces
}

// Trim trailing empty lines
while (lines.length > 0 && lines[lines.length - 1].trim() === '') {
    lines.pop();
}

// ═══════════════════ OUTPUT ═══════════════════

// Print header
console.log(`╔═══════════════════════════════════════════════╗`);
console.log(`║  BHDL Schematic: ${(data.entity_name || 'unknown').substring(0, 28).padEnd(28)} ║`);
console.log(`║  ${layout.elements.length} elements, ${layout.wires.length} wires${' '.repeat(Math.max(0, 22 - String(layout.elements.length).length - String(layout.wires.length).length))} ║`);
console.log(`╚═══════════════════════════════════════════════╝`);
console.log('');

// Print grid
for (const line of lines) {
    console.log(line);
}

// Print legend
console.log('');
console.log('Legend: ┤├=resistor  │├=capacitor  ▷│=diode  ╫/╪=shunt  ▼=GND  ◄►=power');
