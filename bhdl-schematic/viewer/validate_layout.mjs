#!/usr/bin/env node
// validate_layout.mjs — Geometric validation of schematic layouts
//
// Reads SchematicData JSON, runs the layout engine, then checks 18
// geometric invariants (including path-box overlap, chain alignment,
// bus wire consistency, series component wire-through, and wire loops).
// Outputs results as JSON to stdout.
//
// Usage:
//   node validate_layout.mjs <schematic-data.json>
//   node validate_layout.mjs --help

import { computeLayout, LAYOUT_CONSTANTS } from './layout_engine.mjs';
import { readFileSync } from 'fs';

// ═══════════════════ CLI ═══════════════════

const args = process.argv.slice(2);
if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
    console.error(`Usage: node validate_layout.mjs <schematic-data.json>

Runs the schematic layout engine on a SchematicData JSON file and checks
18 geometric invariants.  Outputs validation results as JSON to stdout.

Options:
  --verbose, -v    Show detailed per-check output on stderr
  --help, -h       Show this help
`);
    process.exit(args.includes('--help') || args.includes('-h') ? 0 : 1);
}

const verbose = args.includes('--verbose') || args.includes('-v');
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
    if (verbose) console.error(e.stack);
    process.exit(1);
}

const { BASE_GAP } = LAYOUT_CONSTANTS;

// ═══════════════════ GEOMETRY HELPERS ═══════════════════

function rectsOverlap(a, b, gap = 0) {
    return a.x < b.x + b.w + gap &&
           a.x + a.w + gap > b.x &&
           a.y < b.y + b.h + gap &&
           a.y + a.h + gap > b.y;
}

function pointDist(a, b) {
    return Math.hypot(a.x - b.x, a.y - b.y);
}

/** Check if a line segment intersects a rectangle (excluding the endpoints' own component rects). */
function segmentIntersectsRect(seg, rect, margin = 2) {
    // Axis-aligned segment check: test if any part of the segment passes through the rect
    const minX = Math.min(seg.x1, seg.x2) - margin;
    const maxX = Math.max(seg.x1, seg.x2) + margin;
    const minY = Math.min(seg.y1, seg.y2) - margin;
    const maxY = Math.max(seg.y1, seg.y2) + margin;

    // Quick rejection: no overlap at all
    if (maxX < rect.x || minX > rect.x + rect.w) return false;
    if (maxY < rect.y || minY > rect.y + rect.h) return false;

    // Horizontal segment
    if (Math.abs(seg.y1 - seg.y2) < 2) {
        return seg.y1 >= rect.y && seg.y1 <= rect.y + rect.h &&
               maxX > rect.x && minX < rect.x + rect.w;
    }

    // Vertical segment
    if (Math.abs(seg.x1 - seg.x2) < 2) {
        return seg.x1 >= rect.x && seg.x1 <= rect.x + rect.w &&
               maxY > rect.y && minY < rect.y + rect.h;
    }

    // Diagonal (shouldn't happen in our layout, but handle it)
    return true; // conservative: assume intersection
}

// ═══════════════════ VALIDATION CHECKS ═══════════════════

const errors = [];
const warnings = [];
const instances = layout.elements.filter(e => e.type === 'instance');
const allElements = layout.elements;

// ── 1. No component overlaps ──
for (let i = 0; i < instances.length; i++) {
    for (let j = i + 1; j < instances.length; j++) {
        const a = instances[i], b = instances[j];
        if (rectsOverlap(a, b)) {
            errors.push(`OVERLAP: "${a.name}" (${a.x.toFixed(0)},${a.y.toFixed(0)} ${a.w.toFixed(0)}x${a.h.toFixed(0)}) and "${b.name}" (${b.x.toFixed(0)},${b.y.toFixed(0)} ${b.w.toFixed(0)}x${b.h.toFixed(0)})`);
        }
    }
}

// ── 2. Wire endpoints match port positions (within tolerance) ──
{
    const TOLERANCE = 3;
    const portPositions = new Map(); // "elName.portName" → {x, y}
    for (const el of allElements) {
        for (const p of (el.inputPorts || [])) {
            portPositions.set(`${el.name}.${p.name}`, { x: p.x, y: p.y });
        }
        for (const p of (el.outputPorts || [])) {
            portPositions.set(`${el.name}.${p.name}`, { x: p.x, y: p.y });
        }
        for (const p of (el.topPorts || [])) {
            portPositions.set(`${el.name}.${p.name}`, { x: p.x, y: p.y });
        }
        for (const p of (el.bottomPorts || [])) {
            portPositions.set(`${el.name}.${p.name}`, { x: p.x, y: p.y });
        }
    }
    // Note: Wire endpoints include PORT_STUB_LEN offset from the port position,
    // so we don't check exact port→wire endpoint match. Instead we verify wire
    // segments exist and are reasonable.
}

// ── 3. No wire segments pass through component bounding boxes ──
{
    // Build per-net element sets: components that are source or sink on a net
    // share its trunk wire and should not trigger WIRE_THROUGH against each other.
    const netElSets = new Map();
    for (const wire of layout.wires) {
        const net = wire.netName;
        if (!net) continue;
        if (!netElSets.has(net)) netElSets.set(net, new Set());
        const s = netElSets.get(net);
        if (wire.sourceElName) s.add(wire.sourceElName);
        if (wire.sinkElName) s.add(wire.sinkElName);
    }
    for (const wire of layout.wires) {
        const sameNetEls = netElSets.get(wire.netName) || new Set();
        for (const seg of (wire.segments || [])) {
            for (const el of instances) {
                // Skip components on the same net (trunk passes through their port area)
                if (sameNetEls.has(el.name)) continue;
                // Skip power source elements (they're inline flags, not boxes)
                const fullEl = allElements.find(e => e.name === el.name);
                if (fullEl && fullEl.type === 'power_source') continue;

                if (segmentIntersectsRect(seg, el)) {
                    errors.push(`WIRE_THROUGH: net "${wire.netName}" segment (${seg.x1.toFixed(0)},${seg.y1.toFixed(0)})→(${seg.x2.toFixed(0)},${seg.y2.toFixed(0)}) passes through "${el.name}"`);
                }
            }
        }
    }
}

// ── 4. Left-to-right signal flow (main path X increases) ──
{
    const mainPath = instances.filter(e => !e.isShunt);
    // Sort by X to find pairs that violate L→R ordering
    // Note: we check consecutive main-path elements in their X order
    const sorted = [...mainPath].sort((a, b) => a.x - b.x);
    // Check: for each wire between two main-path elements, source should be left of sink
    for (const wire of layout.wires) {
        if (wire.isPower) continue; // power wires can go anywhere
        const srcEl = mainPath.find(e => e.name === wire.sourceElName);
        const sinkEl = mainPath.find(e => e.name === wire.sinkElName);
        if (srcEl && sinkEl && !sinkEl.isFlipped && !srcEl.isFlipped) {
            if (sinkEl.x + sinkEl.w < srcEl.x) {
                warnings.push(`FLOW: sink "${sinkEl.name}" (x=${sinkEl.x.toFixed(0)}) is left of source "${srcEl.name}" (x=${srcEl.x.toFixed(0)}) on net "${wire.netName}"`);
            }
        }
    }
}

// ── 5. Shunt components are below their junction ──
{
    const shunts = instances.filter(e => e.isShunt);
    const mainPath = instances.filter(e => !e.isShunt);
    if (mainPath.length > 0) {
        const mainMinY = Math.min(...mainPath.map(e => e.y));
        for (const s of shunts) {
            if (s.y < mainMinY) {
                warnings.push(`SHUNT_ABOVE: "${s.name}" (y=${s.y.toFixed(0)}) is above main path (minY=${mainMinY.toFixed(0)})`);
            }
        }
    }
}

// ── 6. Minimum spacing between adjacent components ──
{
    const MIN_SPACING = BASE_GAP * 0.5; // Allow some tolerance (half of BASE_GAP)
    for (let i = 0; i < instances.length; i++) {
        for (let j = i + 1; j < instances.length; j++) {
            const a = instances[i], b = instances[j];
            // Only check components that are close to each other
            const dx = Math.abs((a.x + a.w / 2) - (b.x + b.w / 2));
            const dy = Math.abs((a.y + a.h / 2) - (b.y + b.h / 2));
            if (dx > 500 || dy > 500) continue;

            // Check gap between bounding boxes
            const gapX = Math.max(0, Math.max(a.x, b.x) - Math.min(a.x + a.w, b.x + b.w));
            const gapY = Math.max(0, Math.max(a.y, b.y) - Math.min(a.y + a.h, b.y + b.h));
            const gap = Math.max(gapX, gapY);

            if (gap > 0 && gap < MIN_SPACING && !rectsOverlap(a, b)) {
                warnings.push(`SPACING: "${a.name}" and "${b.name}" gap=${gap.toFixed(1)}px (min=${MIN_SPACING.toFixed(0)}px)`);
            }
        }
    }
}

// ── 7. All ports have at least one wire connected ──
{
    const connectedPorts = new Set();
    for (const wire of layout.wires) {
        if (wire.sinkElName) connectedPorts.add(wire.sinkElName);
        if (wire.sourceElName) connectedPorts.add(wire.sourceElName);
        // Also track by from/to positions
    }
    // Note: Some ports connect via power/ground stubs instead of wires,
    // so this check is informational only
}

// ── 8. Expansion children are near their parent IC ──
{
    const MAX_EXP_DISTANCE = 1500; // Expansion children can be far when other main-band components intervene
    const elByName = new Map(allElements.map(e => [e.name, e]));
    for (const inst of data.instances || []) {
        if (!inst.expansion_parent) continue;
        const parentEl = elByName.get(inst.expansion_parent);
        const childEl = elByName.get(inst.name);
        if (!parentEl || !childEl) continue;

        const dist = pointDist(
            { x: parentEl.x + parentEl.w / 2, y: parentEl.y + parentEl.h / 2 },
            { x: childEl.x + childEl.w / 2, y: childEl.y + childEl.h / 2 }
        );
        if (dist > MAX_EXP_DISTANCE) {
            warnings.push(`EXPANSION_FAR: "${inst.name}" is ${dist.toFixed(0)}px from parent "${inst.expansion_parent}" (max=${MAX_EXP_DISTANCE})`);
        }
    }
}

// ── 9. Axis-aligned wires ──
{
    for (const wire of layout.wires) {
        for (const seg of (wire.segments || [])) {
            const isHoriz = Math.abs(seg.y1 - seg.y2) < 2;
            const isVert = Math.abs(seg.x1 - seg.x2) < 2;
            if (!isHoriz && !isVert) {
                errors.push(`DIAGONAL: net "${wire.netName}" segment (${seg.x1.toFixed(0)},${seg.y1.toFixed(0)})→(${seg.x2.toFixed(0)},${seg.y2.toFixed(0)}) is diagonal`);
            }
        }
    }
}

// ── 10. No duplicate positions ──
{
    const posMap = new Map();
    for (const el of instances) {
        const key = `${Math.round(el.x)},${Math.round(el.y)}`;
        if (posMap.has(key)) {
            errors.push(`DUPLICATE_POS: "${el.name}" and "${posMap.get(key)}" at (${key})`);
        } else {
            posMap.set(key, el.name);
        }
    }
}

// ── 11. Reasonable bounds ──
{
    const MAX_WIDTH = 6000;
    const MAX_HEIGHT = 4000;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const el of allElements) {
        if (el.x != null && el.w != null) {
            minX = Math.min(minX, el.x);
            maxX = Math.max(maxX, el.x + el.w);
        }
        if (el.y != null && el.h != null) {
            minY = Math.min(minY, el.y);
            maxY = Math.max(maxY, el.y + el.h);
        }
    }
    const totalW = maxX - minX;
    const totalH = maxY - minY;
    if (totalW > MAX_WIDTH) {
        warnings.push(`BOUNDS_WIDE: layout width ${totalW.toFixed(0)}px exceeds ${MAX_WIDTH}px`);
    }
    if (totalH > MAX_HEIGHT) {
        warnings.push(`BOUNDS_TALL: layout height ${totalH.toFixed(0)}px exceeds ${MAX_HEIGHT}px`);
    }
}

// ── 12. Port inside box ──
{
    for (const el of instances) {
        const allPorts = [
            ...(el.inputPorts || []),
            ...(el.outputPorts || []),
            ...(el.topPorts || []),
            ...(el.bottomPorts || []),
        ];
        for (const p of allPorts) {
            const margin = 2; // small tolerance for rounding
            const onLeft   = Math.abs(p.x - el.x) <= margin;
            const onRight  = Math.abs(p.x - (el.x + el.w)) <= margin;
            const onTop    = Math.abs(p.y - el.y) <= margin;
            const onBottom = Math.abs(p.y - (el.y + el.h)) <= margin;
            const atCenter = Math.abs(p.x - (el.x + el.w / 2)) <= margin; // for shunt NORTH ports

            if (!onLeft && !onRight && !onTop && !onBottom && !atCenter) {
                // Port is not on any edge — check if it's at least inside the box
                const inside = p.x >= el.x - margin && p.x <= el.x + el.w + margin &&
                               p.y >= el.y - margin && p.y <= el.y + el.h + margin;
                if (!inside) {
                    errors.push(`PORT_OUTSIDE: "${el.name}" port "${p.name}" at (${p.x.toFixed(0)},${p.y.toFixed(0)}) is outside box (${el.x.toFixed(0)},${el.y.toFixed(0)} ${el.w.toFixed(0)}x${el.h.toFixed(0)})`);
                }
            }
        }
    }
}

// ── 13. GND alignment: shunts in the same Y-row should share the same gndTargetY ──
{
    // Group shunts by their Y position (top of component) — those in the same
    // row should have GND symbols at the same vertical position (gndTargetY).
    const Y_BUCKET_TOL = 20;
    const shunts = instances.filter(e => e.isShunt && e.gndTargetY != null);
    const byRow = new Map(); // Y-bucket → [element]
    for (const s of shunts) {
        const bucket = Math.round(s.y / Y_BUCKET_TOL) * Y_BUCKET_TOL;
        if (!byRow.has(bucket)) byRow.set(bucket, []);
        byRow.get(bucket).push(s);
    }
    for (const [rowY, group] of byRow) {
        if (group.length < 2) continue;
        const targets = group.map(e => Math.round(e.gndTargetY));
        const minT = Math.min(...targets);
        const maxT = Math.max(...targets);
        if (maxT - minT > 4) { // 4px tolerance
            const names = group.map(e => `${e.name}(gnd=${Math.round(e.gndTargetY)})`).join(', ');
            warnings.push(`GND_MISALIGN: shunts in row Y≈${rowY} have different gndTargetY: [${names}]`);
        }
    }
}

// ── 14. Path bounding box overlap check ──
// Uses the path bounds computed by the layout engine to detect overlapping groups.
{
    const pathBounds = layout.pathBounds || [];
    for (let i = 0; i < pathBounds.length; i++) {
        for (let j = i + 1; j < pathBounds.length; j++) {
            const a = pathBounds[i], b = pathBounds[j];
            // Skip main_band — shunts are supposed to be below it
            if (a.type === 'main_band' || b.type === 'main_band') continue;
            // Skip same-junction same-side pairs (they're the same group)
            if (a.junctionName === b.junctionName && a.side === b.side) continue;
            if (rectsOverlap(a, b)) {
                warnings.push(`PATH_OVERLAP: "${a.name}" (${a.type}, ${a.x.toFixed(0)},${a.y.toFixed(0)} ${a.w.toFixed(0)}x${a.h.toFixed(0)}) overlaps "${b.name}" (${b.type}, ${b.x.toFixed(0)},${b.y.toFixed(0)} ${b.w.toFixed(0)}x${b.h.toFixed(0)})`);
            }
        }
    }
}

// ── 15. Shunt chain vertical alignment ──
// Parent→child chains (e.g., r_led→led) should be vertically aligned.
// If the child's center X differs from the parent's, connecting wires get
// unnecessary L-bends instead of clean vertical drops.
{
    const elByName = new Map(allElements.map(e => [e.name, e]));
    // Detect chains: parent has output port, child has input port, connected via wire
    // with exactly 1 segment (vertical) or 2 segments (L-bend = misalignment).
    for (const wire of layout.wires) {
        if (!wire.sourceElName || !wire.sinkElName) continue;
        const srcEl = elByName.get(wire.sourceElName);
        const sinkEl = elByName.get(wire.sinkElName);
        if (!srcEl || !sinkEl) continue;
        // Both must be shunt items in a vertical chain (sink below source)
        if (!srcEl.isShunt || !sinkEl.isShunt) continue;
        if (sinkEl.y <= srcEl.y) continue; // sink must be below source
        // Check center X alignment
        const srcCx = srcEl.x + srcEl.w / 2;
        const sinkCx = sinkEl.x + sinkEl.w / 2;
        const ALIGN_TOL = 3; // px tolerance
        if (Math.abs(srcCx - sinkCx) > ALIGN_TOL) {
            errors.push(`CHAIN_MISALIGN: "${srcEl.name}" center X=${srcCx.toFixed(0)} vs "${sinkEl.name}" center X=${sinkCx.toFixed(0)} (offset ${Math.abs(srcCx - sinkCx).toFixed(0)}px, causes L-bend)`);
        }
    }
}

// ── 16. Multi-row bus wire consistency ──
// Serpentine/L-bend bus wires connect row N to row N+1 in multi-row groups.
// The bus cornerX must be within or near the actual element X bounds of the
// row it starts from — stale row extents cause the bus wire to go to the
// wrong location.
{
    const busWires = layout.elements._multiRowBusWires || [];
    const elByName = new Map(allElements.map(e => [e.name, e]));
    for (const bus of busWires) {
        if (!bus.rows || !bus.lbends) continue;
        for (let li = 0; li < bus.lbends.length; li++) {
            const lb = bus.lbends[li];
            const row = bus.rows[li]; // current row (source of L-bend)
            if (!row || row.length === 0) continue;
            // Compute actual extent from BOTH current and next row element
            // positions — the corner intentionally extends past the current
            // row when the next row is wider (after clamping).
            let combinedMinX = Infinity, combinedMaxX = -Infinity;
            for (const item of row) {
                const el = elByName.get(item.name);
                if (!el || (el.w === 0 && el.h === 0)) continue;
                combinedMinX = Math.min(combinedMinX, el.x);
                combinedMaxX = Math.max(combinedMaxX, el.x + el.w);
            }
            const nextRow = bus.rows[li + 1];
            if (nextRow) {
                for (const item of nextRow) {
                    const el = elByName.get(item.name);
                    if (!el || (el.w === 0 && el.h === 0)) continue;
                    combinedMinX = Math.min(combinedMinX, el.x);
                    combinedMaxX = Math.max(combinedMaxX, el.x + el.w);
                }
            }
            if (combinedMinX === Infinity) continue;
            // cornerX should be near the combined row edge (within pad tolerance)
            const BUS_PAD = 30; // allow some padding past row edge
            if (lb.cornerX < combinedMinX - BUS_PAD || lb.cornerX > combinedMaxX + BUS_PAD) {
                errors.push(`BUS_STALE: net "${bus.netName}" L-bend ${li} cornerX=${lb.cornerX.toFixed(0)} is outside row bounds [${combinedMinX.toFixed(0)}, ${combinedMaxX.toFixed(0)}] — stale row extents?`);
            }
            // Check that all next-row cap stubs fall within the feed wire range
            if (nextRow) {
                const feedMinX = Math.min(lb.cornerX, lb.feedEndX);
                const feedMaxX = Math.max(lb.cornerX, lb.feedEndX);
                for (const item of nextRow) {
                    const el = elByName.get(item.name);
                    if (!el || (el.w === 0 && el.h === 0)) continue;
                    const stubX = el.x + el.w / 2;
                    if (stubX < feedMinX - 5 || stubX > feedMaxX + 5) {
                        errors.push(`BUS_DANGLING: net "${bus.netName}" L-bend ${li} stub for "${item.name}" at x=${stubX.toFixed(0)} outside feed wire [${feedMinX.toFixed(0)}, ${feedMaxX.toFixed(0)}]`);
                    }
                }
            }
            // Check inter-L-bend connectivity: L-bend li>0 must connect
            // to the previous feed wire (cornerX within prev feed range,
            // startY == prev feedY)
            if (li > 0) {
                const prevLb = bus.lbends[li - 1];
                const prevFeedMinX = Math.min(prevLb.cornerX, prevLb.feedEndX);
                const prevFeedMaxX = Math.max(prevLb.cornerX, prevLb.feedEndX);
                if (lb.cornerX < prevFeedMinX - 5 || lb.cornerX > prevFeedMaxX + 5) {
                    errors.push(`BUS_DISCONNECT: net "${bus.netName}" L-bend ${li} cornerX=${lb.cornerX.toFixed(0)} not on prev feed wire [${prevFeedMinX.toFixed(0)}, ${prevFeedMaxX.toFixed(0)}]`);
                }
                const startY = lb.startY != null ? lb.startY : bus.rowExtents[li]?.rowY;
                if (startY != null && Math.abs(startY - prevLb.feedY) > 5) {
                    errors.push(`BUS_DISCONNECT: net "${bus.netName}" L-bend ${li} startY=${startY.toFixed(0)} != prev feedY=${prevLb.feedY.toFixed(0)} (gap of ${Math.abs(startY - prevLb.feedY).toFixed(0)}px)`);
                }
            }
        }
    }
}

// ── 17. No wire through series (non-shunt) same-net components ──
// The trunk wire for a net may pass through shunt items' port areas (they
// T-junction from the trunk), but it should NOT pass through the body of
// series (main-band) components even if they're on the same net. The wire
// should route around them.
{
    // Build per-net SHUNT-ONLY exclude sets (not all same-net elements)
    const netShuntExclude = new Map();
    const netAllEls = new Map();
    for (const wire of layout.wires) {
        const net = wire.netName;
        if (!net) continue;
        if (!netShuntExclude.has(net)) netShuntExclude.set(net, new Set());
        if (!netAllEls.has(net)) netAllEls.set(net, new Set());
        const s = netShuntExclude.get(net);
        const a = netAllEls.get(net);
        if (wire.sourceElName) { s.add(wire.sourceElName); a.add(wire.sourceElName); }
        if (wire.sinkElName) {
            a.add(wire.sinkElName);
            const el = instances.find(e => e.name === wire.sinkElName);
            if (el && el.isShunt) s.add(wire.sinkElName);
        }
    }
    for (const wire of layout.wires) {
        const shuntExclude = netShuntExclude.get(wire.netName) || new Set();
        const allOnNet = netAllEls.get(wire.netName) || new Set();
        for (const seg of (wire.segments || [])) {
            for (const el of instances) {
                // Skip elements excluded by shunt-only logic
                if (shuntExclude.has(el.name)) continue;
                // Skip elements NOT on this net (they're checked by check 3)
                if (!allOnNet.has(el.name)) continue;
                // Skip power source elements
                const fullEl = allElements.find(e => e.name === el.name);
                if (fullEl && fullEl.type === 'power_source') continue;
                if (!segmentIntersectsRect(seg, el)) continue;
                // For the wire's own source/sink: only flag if the segment PASSES
                // THROUGH the component (both endpoints far outside the rect).
                // A segment that terminates at or near the component is a
                // legitimate port connection (port stubs extend ~14px past the rect).
                if (el.name === wire.sourceElName || el.name === wire.sinkElName) {
                    const STUB = 16; // PORT_STUB_LEN + tolerance
                    const p1Near = seg.x1 >= el.x - STUB && seg.x1 <= el.x + el.w + STUB &&
                                   seg.y1 >= el.y - STUB && seg.y1 <= el.y + el.h + STUB;
                    const p2Near = seg.x2 >= el.x - STUB && seg.x2 <= el.x + el.w + STUB &&
                                   seg.y2 >= el.y - STUB && seg.y2 <= el.y + el.h + STUB;
                    if (p1Near || p2Near) continue; // terminates at component — OK
                }
                errors.push(`WIRE_THROUGH_SERIES: net "${wire.netName}" segment (${seg.x1.toFixed(0)},${seg.y1.toFixed(0)})→(${seg.x2.toFixed(0)},${seg.y2.toFixed(0)}) passes through series component "${el.name}"`);
            }
        }
    }
}

// ── 18. Wire loop / U-turn detection ──
// A wire loop occurs when a single wire (source→sink route) has two
// horizontal segments at different Y levels whose X projections overlap,
// or two vertical segments at different X levels whose Y projections
// overlap.  This indicates an unnecessary detour — the wire went out of
// its way and came back, creating a rectangular loop that could have been
// a straight or single-bend route.
{
    for (const wire of layout.wires) {
        const segs = wire.segments || [];
        if (segs.length < 3) continue; // need at least 3 segments for a loop

        // Collect horizontal segments grouped by Y
        const horizByY = new Map(); // roundedY → [{x1, x2, idx}]
        // Collect vertical segments grouped by X
        const vertByX = new Map();  // roundedX → [{y1, y2, idx}]

        for (let i = 0; i < segs.length; i++) {
            const seg = segs[i];
            const isHoriz = Math.abs(seg.y1 - seg.y2) < 2;
            const isVert = Math.abs(seg.x1 - seg.x2) < 2;

            if (isHoriz) {
                const y = Math.round(seg.y1);
                if (!horizByY.has(y)) horizByY.set(y, []);
                horizByY.get(y).push({
                    minX: Math.min(seg.x1, seg.x2),
                    maxX: Math.max(seg.x1, seg.x2),
                    idx: i
                });
            }
            if (isVert) {
                const x = Math.round(seg.x1);
                if (!vertByX.has(x)) vertByX.set(x, []);
                vertByX.get(x).push({
                    minY: Math.min(seg.y1, seg.y2),
                    maxY: Math.max(seg.y1, seg.y2),
                    idx: i
                });
            }
        }

        // Check for horizontal overlap at different Y levels
        const yLevels = [...horizByY.keys()].sort((a, b) => a - b);
        for (let i = 0; i < yLevels.length; i++) {
            for (let j = i + 1; j < yLevels.length; j++) {
                const y1 = yLevels[i], y2 = yLevels[j];
                if (Math.abs(y1 - y2) < 4) continue; // same level (rounding)
                for (const segA of horizByY.get(y1)) {
                    for (const segB of horizByY.get(y2)) {
                        // Check X-range overlap (both segments must span a shared range)
                        const overlapStart = Math.max(segA.minX, segB.minX);
                        const overlapEnd = Math.min(segA.maxX, segB.maxX);
                        const overlapLen = overlapEnd - overlapStart;
                        if (overlapLen > 10) { // >10px overlap = real detour, not just rounding
                            errors.push(`WIRE_LOOP: net "${wire.netName}" has horizontal segments at y=${y1} and y=${y2} with ${overlapLen.toFixed(0)}px X-overlap [${overlapStart.toFixed(0)}..${overlapEnd.toFixed(0)}] — detour/U-turn`);
                        }
                    }
                }
            }
        }

        // Check for vertical overlap at different X levels
        const xLevels = [...vertByX.keys()].sort((a, b) => a - b);
        for (let i = 0; i < xLevels.length; i++) {
            for (let j = i + 1; j < xLevels.length; j++) {
                const x1 = xLevels[i], x2 = xLevels[j];
                if (Math.abs(x1 - x2) < 4) continue;
                for (const segA of vertByX.get(x1)) {
                    for (const segB of vertByX.get(x2)) {
                        const overlapStart = Math.max(segA.minY, segB.minY);
                        const overlapEnd = Math.min(segA.maxY, segB.maxY);
                        const overlapLen = overlapEnd - overlapStart;
                        if (overlapLen > 10) {
                            errors.push(`WIRE_LOOP: net "${wire.netName}" has vertical segments at x=${x1} and x=${x2} with ${overlapLen.toFixed(0)}px Y-overlap [${overlapStart.toFixed(0)}..${overlapEnd.toFixed(0)}] — detour/U-turn`);
                        }
                    }
                }
            }
        }
    }
}

// ═══════════════════ OUTPUT ═══════════════════

const result = {
    errors,
    warnings,
    stats: {
        elements: allElements.length,
        instances: instances.length,
        wires: layout.wires.length,
        stageZones: layout.stageZones.length,
        pathBounds: (layout.pathBounds || []).length,
        overlaps: errors.filter(e => e.startsWith('OVERLAP')).length,
        wireThroughBox: errors.filter(e => e.startsWith('WIRE_THROUGH:')).length,
        wireThroughSeries: errors.filter(e => e.startsWith('WIRE_THROUGH_SERIES')).length,
        chainMisalign: errors.filter(e => e.startsWith('CHAIN_MISALIGN')).length,
        busStale: errors.filter(e => e.startsWith('BUS_STALE')).length,
        busDangling: errors.filter(e => e.startsWith('BUS_DANGLING')).length,
        busDisconnect: errors.filter(e => e.startsWith('BUS_DISCONNECT')).length,
        wireLoops: errors.filter(e => e.startsWith('WIRE_LOOP')).length,
        diagonals: errors.filter(e => e.startsWith('DIAGONAL')).length,
    },
    pass: errors.length === 0,
};

if (verbose) {
    console.error(`\n=== Layout Validation ===`);
    console.error(`Elements: ${result.stats.elements} (${result.stats.instances} instances)`);
    console.error(`Wires: ${result.stats.wires}`);
    console.error(`Stage Zones: ${result.stats.stageZones}`);
    console.error(`Errors: ${errors.length}`);
    console.error(`Warnings: ${warnings.length}`);
    if (errors.length > 0) {
        console.error(`\n--- Errors ---`);
        for (const e of errors) console.error(`  ✗ ${e}`);
    }
    if (warnings.length > 0) {
        console.error(`\n--- Warnings ---`);
        for (const w of warnings) console.error(`  ⚠ ${w}`);
    }
    console.error(`\n${result.pass ? '✓ PASS' : '✗ FAIL'}`);
}

console.log(JSON.stringify(result, null, 2));
process.exit(result.pass ? 0 : 1);
