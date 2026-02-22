// BHDL Schematic Viewer — Canvas-based circuit visualization with custom layout
// Conventions: signal flow left-to-right, power north, ground south
(function () {
    let vscodeApi = null;
    try { vscodeApi = acquireVsCodeApi(); } catch (e) { /* standalone */ }

    const canvas = /** @type {HTMLCanvasElement} */ (document.getElementById('schematic-canvas'));
    const ctx = canvas.getContext('2d');
    const entityNameEl = document.getElementById('entity-name');
    const statsEl = document.getElementById('stats');
    const debugPanel = document.getElementById('debug-panel');
    const debugChk = document.getElementById('chk-debug');
    if (debugChk) debugChk.addEventListener('change', () => {
        if (debugPanel) debugPanel.style.display = debugChk.checked ? 'block' : 'none';
    });
    const refdesChk = document.getElementById('chk-refdes');
    if (refdesChk) refdesChk.addEventListener('change', () => {
        // Update displayName on all instance elements and re-render
        const useRefDes = refdesChk.checked;
        for (const el of layoutElements) {
            if (el.type === 'instance' && el.refdes) {
                el.displayName = useRefDes ? el.refdes : el.name;
            }
        }
        render();
    });

    let schematicData = null;
    let panX = 0, panY = 0;
    let zoomLevel = 1.0;
    let hoveredItem = null;
    let hoveredNet = null;

    // Layout constants
    const PORT_SPACING = 24;
    const PORT_STUB_LEN = 14;
    const ENTITY_PADDING = 16;
    const INSTANCE_PADDING = 12;
    const BASE_GAP = PORT_STUB_LEN * 2 + 16;  // 44px — minimum gap (stubs + margin)
    const MAX_GAP = 200;                       // cap to prevent overly long wires
    const ANNOTATION_PAD = 12;                 // extra padding around annotation text
    const ROW_GAP = 50;
    const HEADER_HEIGHT = 22;
    const FONT_SIZE = 11;
    const PORT_DOT_R = 3;
    const ENTITY_BOX_MIN_WIDTH = 180;
    const INSTANCE_BOX_MIN_WIDTH = 120;
    const BORDER_RADIUS = 5;
    const PARAM_ROW_HEIGHT = 14;
    const GND_STUB_HEIGHT = 18;
    const GND_LINE_WIDTHS = [12, 8, 4];
    const GND_LINE_SPACING = 3;

    // Only these keys are shown inline on the component; everything else is hover-only
    const INLINE_PARAM_KEYS = new Set(['value']);

    // Professional symbol sizes: { bodyW, bodyH } = drawn symbol,
    // { boundW, boundH } = total bounding box including lead stubs & label space
    const SYMBOL_SIZES = {
        resistor:   { bodyW: 40, bodyH: 12, boundW: 68, boundH: 44 },
        capacitor:  { bodyW:  6, bodyH: 20, boundW: 34, boundH: 44 },
        inductor:   { bodyW: 36, bodyH: 12, boundW: 64, boundH: 44 },
        diode:      { bodyW: 10, bodyH: 14, boundW: 56, boundH: 44 },
        protection: { bodyW: 12, bodyH: 14, boundW: 60, boundH: 44 },
        opamp:      { bodyW: 50, bodyH: 44, boundW: 78, boundH: 60 },
    };

    function isSymbolCategory(cat) {
        return cat in SYMBOL_SIZES;
    }

    const COLORS = {
        entityBg: '#1e3a5f', entityBorder: '#4fc3f7', entityHeader: '#0d2137',
        instanceBg: '#2a2d2e', instanceBorder: '#555', instanceHeader: '#383b3d',
        port: '#5c8dbf',           // signal port stubs and dots
        portPower: '#ff6b6b',      // power port stubs
        portClock: '#4caf50',      // clock signal ports
        portReset: '#ef5350',      // reset signal ports
        wire: '#5c8dbf', wireBus: '#7baad4',
        wireHighlight: '#ffeb3b',
        text: '#d4d4d4', textDim: '#777', textMuted: '#555',
        paramText: '#9e9e9e',
        busSlash: '#8cb4d8', busLabel: '#8cb4d8',
        highlight: '#ffeb3b', junctionDot: '#5c8dbf',
        powerSrcBg: '#3a1818', powerSrcBorder: '#ff6b6b', powerSrcText: '#ff6b6b',
        groundStub: '#888888'
    };

    let clockSignals = new Set();
    let resetSignals = new Set();
    let layoutElements = [];
    let layoutWires = [];

    function resizeCanvas() {
        const w = canvas.clientWidth, h = canvas.clientHeight;
        if (w <= 0 || h <= 0) return;
        const dpr = window.devicePixelRatio || 1;
        canvas.width = w * dpr;
        canvas.height = h * dpr;
        render();
    }

    // ─────────── HELPERS ───────────

    /** Format a parameter value using engineering notation (10k, 1M, 100n, etc.) */
    function formatParamValue(val) {
        if (val == null || val === '') return '';
        // Already has a unit suffix or is non-numeric — return as-is
        if (/[a-zA-Zµμ]/.test(val)) return val;
        const num = parseFloat(val);
        if (isNaN(num)) return val;
        const abs = Math.abs(num);
        if (abs === 0) return '0';
        if (abs >= 1e12) return (num / 1e12) + 'T';
        if (abs >= 1e9)  return (num / 1e9) + 'G';
        if (abs >= 1e6)  return (num / 1e6) + 'M';
        if (abs >= 1e3)  return (num / 1e3) + 'k';
        if (abs >= 1)    return String(num);
        if (abs >= 1e-3) return (num * 1e3) + 'm';
        if (abs >= 1e-6) return (num * 1e6) + 'u';
        if (abs >= 1e-9) return (num * 1e9) + 'n';
        if (abs >= 1e-12) return (num * 1e12) + 'p';
        return num.toExponential(1);
    }

    function formatVoltage(v) {
        if (v == null) return '';
        const abs = Math.abs(v);
        if (abs >= 1) return v.toFixed(2) + 'V';
        if (abs >= 1e-3) return (v * 1e3).toFixed(1) + 'mV';
        return (v * 1e6).toFixed(0) + 'µV';
    }

    function formatCurrent(a) {
        if (a == null) return '';
        const abs = Math.abs(a);
        if (abs >= 1) return a.toFixed(2) + 'A';
        if (abs >= 1e-3) return (a * 1e3).toFixed(1) + 'mA';
        if (abs >= 1e-6) return (a * 1e6).toFixed(1) + 'µA';
        return (a * 1e9).toFixed(0) + 'nA';
    }

    function formatPower(w) {
        if (w == null) return '';
        const abs = Math.abs(w);
        if (abs >= 1) return w.toFixed(2) + 'W';
        if (abs >= 1e-3) return (w * 1e3).toFixed(1) + 'mW';
        if (abs >= 1e-6) return (w * 1e6).toFixed(1) + 'µW';
        return (w * 1e9).toFixed(0) + 'nW';
    }

    function isPowerGroundSymbol(inst) {
        const t = inst.entity_type || '';
        if (t === 'GND' || t === 'VSS' || t === 'AGND' || t === 'DGND') return true;
        if (/^[+-]\d/.test(t)) return true;
        if (t === 'VCC' || t === 'VDD' || t === 'VBUS') return true;
        if (inst.connections.length <= 1 &&
            inst.connections.every(c => c.pin_type === 'power' || c.pin_type === 'ground')) return true;
        return false;
    }

    /** Should we show port labels for this component category? */
    function shouldShowPortLabels(category) {
        return category === 'ic' || category === 'regulator' || category === 'buffer'
            || category === 'oscillator' || category === 'connector';
    }

    function getPortPinType(instName, portName) {
        if (!schematicData) return 'signal';
        const inst = schematicData.instances.find(i => i.name === instName);
        if (!inst) return 'signal';
        const conn = inst.connections.find(c => c.port === portName);
        return conn ? (conn.pin_type || 'signal') : 'signal';
    }

    // ─────────── LAYOUT ───────────

    function measureTextWidth(text, fontSize) {
        return text.length * (fontSize * 0.62);
    }

    function computeBoxSize(name, entityType, inPorts, outPorts, parameters, category) {
        if (category && isSymbolCategory(category)) {
            const s = SYMBOL_SIZES[category];
            return { w: s.boundW, h: s.boundH };
        }
        const portCount = Math.max(inPorts.length, outPorts.length, 1);
        const paramLines = (parameters && parameters.length > 0) ? 1 : 0;
        const h = HEADER_HEIGHT + INSTANCE_PADDING * 2 + portCount * PORT_SPACING + paramLines * PARAM_ROW_HEIGHT;
        let maxIn = 0, maxOut = 0;
        for (const p of inPorts) maxIn = Math.max(maxIn, measureTextWidth(p, FONT_SIZE));
        for (const p of outPorts) maxOut = Math.max(maxOut, measureTextWidth(p, FONT_SIZE));
        const displayLabel = entityType || name;
        const nameW = measureTextWidth(displayLabel, FONT_SIZE + 1);
        const w = Math.max(INSTANCE_BOX_MIN_WIDTH, maxIn + maxOut + 40, nameW + 30);
        return { w, h };
    }

    function computeLayout() {
        if (!schematicData) return;
        layoutElements = [];
        layoutWires = [];
        const data = schematicData;

        // ── 1. Identify PG symbols and GND nets ──
        const pgInstNames = new Set();
        const gndNetNames = new Set();
        for (const inst of data.instances) {
            if (isPowerGroundSymbol(inst)) pgInstNames.add(inst.name);
        }
        for (const net of (data.nets || [])) {
            if (net.net_class === 'ground') gndNetNames.add(net.name);
        }

        // ── 2. Process nets: remove PG symbols, create synthetic power sources ──
        const powerSourceNodes = [];
        let processedNets = [];
        for (const net of (data.nets || [])) {
            if (net.net_class === 'ground') continue;
            let driver = { ...net.driver };
            let sinks = net.sinks.map(s => ({ ...s }));
            const driverIsPg = pgInstNames.has(driver.name);
            sinks = sinks.filter(s => !pgInstNames.has(s.name));
            if (driverIsPg) {
                // Promote regulator output to driver
                const regIdx = sinks.findIndex(s => {
                    const inst = data.instances.find(i => i.name === s.name);
                    if (!inst) return false;
                    const up = s.port.toUpperCase();
                    if (!(up === 'VO' || up === 'VOUT' || up === 'OUT' || up === 'OUTPUT')) return false;
                    if (inst.category === 'regulator') return true;
                    // Fallback: detect regulator by pin structure (has both VI/VIN and VO/VOUT)
                    const hasInputPin = inst.connections.some(c => {
                        const p = c.port.toUpperCase();
                        return p === 'VI' || p === 'VIN' || p === 'IN' || p === 'INPUT';
                    });
                    return hasInputPin;
                });
                if (regIdx >= 0) {
                    driver = sinks.splice(regIdx, 1)[0];
                } else if (sinks.length > 0) {
                    const simV = data.simulation?.net_voltages?.[net.name];
                    const displayV = net.voltage ?? simV;
                    const label = displayV != null ? `${net.name} (${formatVoltage(displayV)})` : net.name;
                    const sourceId = `__pwr_${net.name}__`;
                    powerSourceNodes.push({ id: sourceId, label, voltage: displayV });
                    driver = { type: 'power_source', name: sourceId, port: 'out' };
                } else { continue; }
            }
            if (sinks.length === 0) continue;
            processedNets.push({ name: net.name, width: net.width || 1, net_class: net.net_class, voltage: net.voltage, driver, sinks });
        }

        // ── 3. Shunt detection + net merging ──
        const shuntPinKeys = new Set();
        for (const inst of data.instances) {
            if (pgInstNames.has(inst.name)) continue;
            const signalPorts = new Set();
            const gndPorts = new Set();
            for (const c of inst.connections) {
                if (gndNetNames.has(c.signal)) gndPorts.add(c.port); else signalPorts.add(c.port);
            }
            if (signalPorts.size === 1 && gndPorts.size >= 1) shuntPinKeys.add(`${inst.name}.${signalPorts.values().next().value}`);
        }
        const shuntInstNames = new Set();
        for (const k of shuntPinKeys) shuntInstNames.add(k.split('.')[0]);

        let mergeChanged = true;
        while (mergeChanged) {
            mergeChanged = false;
            for (const pinKey of shuntPinKeys) {
                const downIdx = processedNets.findIndex(n => n && n.driver.type !== 'power_source' && `${n.driver.name}.${n.driver.port}` === pinKey);
                if (downIdx < 0) continue;
                const upIdx = processedNets.findIndex(n => n && n.sinks.some(s => `${s.name}.${s.port}` === pinKey));
                if (upIdx < 0 || upIdx === downIdx) continue;
                processedNets[upIdx].sinks = [...processedNets[upIdx].sinks, ...processedNets[downIdx].sinks];
                processedNets.splice(downIdx, 1);
                mergeChanged = true;
                break;
            }
        }

        // ── 3b. Shunt chain extension ──
        // When a dual-port component (2 signal ports, no GND) feeds directly
        // into a shunt, both form a vertical shunt chain (power → R → LED → GND).
        // Reclassify the upstream component as a shunt so the pair stacks vertically.
        const shuntChainDown = new Map(); // parentName → childName
        {
            let extended = true;
            while (extended) {
                extended = false;
                for (const inst of data.instances) {
                    if (pgInstNames.has(inst.name)) continue;
                    if (shuntInstNames.has(inst.name)) continue;
                    // Never chain-promote series expansion children (e.g., inductor
                    // from virtual pin expansion) — they must stay inline.
                    if (inst.expansion_role === 'series') continue;
                    const signalPorts = new Set();
                    const gndPorts = new Set();
                    for (const c of inst.connections) {
                        if (gndNetNames.has(c.signal)) gndPorts.add(c.port);
                        else signalPorts.add(c.port);
                    }
                    if (signalPorts.size !== 2 || gndPorts.size > 0) continue;

                    // Check if this component drives a shunt
                    // But don't chain-promote if the component's input comes directly
                    // from a power source — that means it's the first inline component
                    // (e.g., top resistor in a voltage divider), not a branch.
                    let inputFromPowerSource = false;
                    for (const net of processedNets) {
                        if (net.sinks.some(s => s.name === inst.name)) {
                            if (net.driver.type === 'power_source' || pgInstNames.has(net.driver.name)) {
                                inputFromPowerSource = true;
                                break;
                            }
                        }
                    }
                    if (inputFromPowerSource) continue;

                    for (const net of processedNets) {
                        if (net.driver.name !== inst.name) continue;
                        const shuntSink = net.sinks.find(s => shuntInstNames.has(s.name));
                        if (!shuntSink) continue;
                        const driverPort = net.driver.port;
                        const inputPort = [...signalPorts].find(p => p !== driverPort);
                        if (!inputPort) continue;
                        shuntInstNames.add(inst.name);
                        shuntPinKeys.add(`${inst.name}.${inputPort}`);
                        shuntChainDown.set(inst.name, shuntSink.name);
                        extended = true;
                        break;
                    }
                    if (extended) break;
                }
            }
        }

        // ── 4. Collect GND stubs ──
        const gndStubsByInst = new Map();
        for (const net of (data.nets || [])) {
            if (net.net_class !== 'ground') continue;
            for (const ep of [net.driver, ...net.sinks]) {
                if (!ep || ep.type === 'entity_port' || pgInstNames.has(ep.name)) continue;
                if (!gndStubsByInst.has(ep.name)) gndStubsByInst.set(ep.name, []);
                const arr = gndStubsByInst.get(ep.name);
                if (!arr.some(s => s.port === ep.port)) arr.push({ port: ep.port });
            }
        }

        // ── 4b. Collect power (VCC) stubs ──
        // Only power INPUT pins get stubs — output power pins (e.g., regulator VO)
        // are producers, not consumers, so they don't get a supply stub.
        // Regulators are skipped entirely: their power pins ARE the main signal
        // path (VIN→VOUT), not secondary supply connections.
        const pwrStubsByInst = new Map();
        for (const net of (data.nets || [])) {
            if (net.net_class !== 'power') continue;
            for (const ep of net.sinks) {
                if (!ep || ep.type === 'entity_port' || pgInstNames.has(ep.name)) continue;
                const inst = data.instances.find(i => i.name === ep.name);
                if (!inst) continue;
                // Regulators' power pins are inline — skip entirely
                if (inst.category === 'regulator') continue;
                const conn = inst.connections.find(c => c.port === ep.port);
                if (!conn || conn.pin_type !== 'power') continue;
                // Skip power output pins — they produce the rail, not consume it
                if (conn.pin_direction === 'out') continue;
                if (!pwrStubsByInst.has(ep.name)) pwrStubsByInst.set(ep.name, []);
                const arr = pwrStubsByInst.get(ep.name);
                if (!arr.some(s => s.port === ep.port)) arr.push({ port: ep.port, netName: net.name, voltage: net.voltage });
            }
        }

        // ── 5. Build port role map (inst.port → 'in'/'out' from processed nets) ──
        const instPortRoles = new Map();
        for (const net of processedNets) {
            if (net.driver.type !== 'power_source') {
                const dk = `${net.driver.name}.${net.driver.port}`;
                if (!instPortRoles.has(dk)) instPortRoles.set(dk, new Set());
                instPortRoles.get(dk).add('out');
            }
            for (const s of net.sinks) {
                const sk = `${s.name}.${s.port}`;
                if (!instPortRoles.has(sk)) instPortRoles.set(sk, new Set());
                instPortRoles.get(sk).add('in');
            }
        }

        // ── 5b. Fix port roles for series expansion children ──
        // A series expansion child (e.g., inductor) has its output pin
        // connected to a power net where the PG symbol is the driver.
        // The net role makes both pins "in", but the declared pin direction
        // says the output pin is "out".  Override so the component gets
        // left/right ports and routes inline.
        for (const inst of data.instances) {
            if (inst.expansion_role !== 'series') continue;
            for (const c of inst.connections) {
                if (c.pin_direction === 'out') {
                    const k = `${inst.name}.${c.port}`;
                    // Replace the "in" role with "out" (don't keep both)
                    instPortRoles.set(k, new Set(['out']));
                }
            }
        }

        // ── 6. Classify instances by placement role ──
        const instMap = new Map();
        for (const inst of data.instances) {
            if (!pgInstNames.has(inst.name)) instMap.set(inst.name, inst);
        }

        // ── 6a-exp. Build expansion group map ──
        // Groups virtual-pin expanded components by their parent regulator.
        // expansionGroups: parentName → { series: [names], shunt: [names] }
        const expansionGroups = new Map();
        for (const [name, inst] of instMap) {
            if (inst.expansion_parent) {
                if (!expansionGroups.has(inst.expansion_parent)) {
                    expansionGroups.set(inst.expansion_parent, { series: [], shunt: [] });
                }
                const group = expansionGroups.get(inst.expansion_parent);
                if (inst.expansion_role === 'series') group.series.push(name);
                else if (inst.expansion_role === 'shunt') group.shunt.push(name);
            }
        }

        const mainPathNames = new Set();
        let shuntNames = [];        // {name, junctionName, junctionSide}
        const decouplingNames = [];  // {name, junctionName, junctionSide}
        let branchNames = [];        // {name}

        for (const [name, inst] of instMap) {
            const role = inst.placement_role;
            if (role === 'shunt' || (!role && shuntInstNames.has(name)) || (!role && shuntChainDown.has(name))) {
                shuntNames.push({ name });
            } else if (role === 'branch') {
                branchNames.push({ name });
            } else if (role && typeof role === 'object' && role.decoupling != null) {
                decouplingNames.push({ name, adjacentTo: role.decoupling.adjacent_to || '' });
            } else {
                mainPathNames.add(name);
            }
        }

        // ── 6b. Extend main path through inline branch components ──
        // A branch component with both signal inputs (from main-path) and signal outputs
        // (to other components) is topologically inline — promote it to main path.
        // This handles cases like series sense resistors classified as "branch" by intent.
        {
            let promoted = true;
            while (promoted) {
                promoted = false;
                for (let i = branchNames.length - 1; i >= 0; i--) {
                    const bName = branchNames[i].name;
                    let hasInputFromMain = false;
                    let hasSignalOutput = false;
                    for (const net of processedNets) {
                        // Check if this branch component receives input from a main-path or power source
                        if (net.sinks.some(s => s.name === bName)) {
                            if (mainPathNames.has(net.driver.name) || net.driver.type === 'power_source') {
                                hasInputFromMain = true;
                            }
                        }
                        // Check if this branch component drives a non-shunt component
                        // (shunt-only outputs mean this is a branch head, not inline)
                        if (net.driver.name === bName && net.sinks.length > 0) {
                            if (net.sinks.some(s => !shuntInstNames.has(s.name))) {
                                hasSignalOutput = true;
                            }
                        }
                    }
                    if (hasInputFromMain && hasSignalOutput) {
                        mainPathNames.add(bName);
                        branchNames.splice(i, 1);
                        promoted = true;
                    }
                }
            }
        }

        // ── 7. Order main path by traversal through graph ──
        // Build adjacency including power sources, but only traverse to main path nodes
        const allForward = new Map(); // name → [name] (neighbors via processed nets)
        for (const net of processedNets) {
            const dName = net.driver.name;
            for (const s of net.sinks) {
                if (!allForward.has(dName)) allForward.set(dName, []);
                allForward.get(dName).push(s.name);
            }
        }

        // BFS from power sources through main-path nodes
        const mainPathOrder = [];
        const mpVisited = new Set();
        // Seed: power source nodes, then entity input ports
        const seeds = powerSourceNodes.map(ps => ps.id);
        // Also add main-path nodes that are driven only by power sources or entity ports
        for (const net of processedNets) {
            if (net.driver.type === 'power_source' || net.driver.type === 'entity_port') {
                for (const s of net.sinks) {
                    if (mainPathNames.has(s.name)) seeds.push(s.name);
                }
            }
        }
        const queue = [...seeds];
        while (queue.length > 0) {
            const cur = queue.shift();
            if (mpVisited.has(cur)) continue;
            mpVisited.add(cur);
            if (mainPathNames.has(cur)) mainPathOrder.push(cur);
            for (const neighbor of (allForward.get(cur) || [])) {
                if (mainPathNames.has(neighbor) && !mpVisited.has(neighbor)) queue.push(neighbor);
            }
        }
        // Add any remaining main path nodes not reached
        for (const name of mainPathNames) {
            if (!mpVisited.has(name)) mainPathOrder.push(name);
        }

        // ── 7c. Detect parallel branches (fan-out to multiple main-path sinks) ──
        // When a net drives multiple main-path components through SIGNAL pins,
        // they form parallel branches. Keep the one with the longest forward chain
        // on the main path; demote the rest to branches.
        const parallelBranchJunctions = new Map(); // demotedName → keepName
        {
            // Count forward-reachable main-path nodes (excluding driver to avoid cycles)
            function forwardMainPathDepth(startName, driverName) {
                const visited = new Set();
                const q = [startName];
                let depth = 0;
                let cyclesToDriver = false;
                while (q.length > 0) {
                    const cur = q.shift();
                    for (const n of (allForward.get(cur) || [])) {
                        if (n === driverName) { cyclesToDriver = true; continue; }
                        if (mainPathNames.has(n) && !visited.has(n)) {
                            visited.add(n);
                            depth++;
                            q.push(n);
                        }
                    }
                }
                return { depth, cyclesToDriver };
            }

            for (const net of processedNets) {
                // Only consider signal-pin sinks (filter out power/ground connections like VCC)
                const mainSinks = net.sinks
                    .filter(s => {
                        if (!mainPathNames.has(s.name)) return false;
                        const pinType = getPortPinType(s.name, s.port);
                        return pinType !== 'power' && pinType !== 'ground';
                    })
                    .map(s => s.name);
                if (mainSinks.length <= 1) continue;
                // Never demote series expansion children — they must stay inline
                // with their parent on the main path.
                const nonDemotable = new Set();
                for (const n of mainSinks) {
                    const inst = instMap.get(n);
                    if (inst && inst.expansion_role === 'series') nonDemotable.add(n);
                }

                // Score each sink: longest forward chain wins, prefer non-cyclic paths
                const driverName = net.driver.type === 'power_source' ? null : net.driver.name;
                const scored = mainSinks.map(name => {
                    const { depth, cyclesToDriver } = forwardMainPathDepth(name, driverName);
                    return { name, depth, cyclesToDriver };
                });
                scored.sort((a, b) => {
                    if (b.depth !== a.depth) return b.depth - a.depth;
                    if (a.cyclesToDriver !== b.cyclesToDriver) return a.cyclesToDriver ? 1 : -1;
                    return mainPathOrder.indexOf(a.name) - mainPathOrder.indexOf(b.name);
                });
                const keepName = scored[0].name;

                for (let i = 1; i < scored.length; i++) {
                    const demotedName = scored[i].name;
                    if (nonDemotable.has(demotedName)) continue;
                    mainPathNames.delete(demotedName);
                    const idx = mainPathOrder.indexOf(demotedName);
                    if (idx >= 0) mainPathOrder.splice(idx, 1);
                    parallelBranchJunctions.set(demotedName, keepName);
                    branchNames.push({
                        name: demotedName,
                        junctionName: keepName,
                        junctionSide: 'left',
                        isParallel: true
                    });
                }
            }

            // Move downstream shunts of demoted components into the branch group
            for (const [demotedName, keepName] of parallelBranchJunctions) {
                for (let si = shuntNames.length - 1; si >= 0; si--) {
                    const shunt = shuntNames[si];
                    const isDriven = processedNets.some(net =>
                        net.driver.name === demotedName &&
                        net.sinks.some(s => s.name === shunt.name)
                    );
                    if (isDriven) {
                        shuntNames.splice(si, 1);
                        branchNames.push({
                            name: shunt.name,
                            junctionName: keepName,
                            junctionSide: 'left',
                            isParallel: true
                        });
                    }
                }
            }
        }

        // ── 7d. Demote dead-end main-path loads to shunts ──
        // When a net has 2+ non-PG sinks and a main-path sink whose forward chain
        // doesn't reach other main-path nodes (depth 0), it's a load branch
        // (e.g., r_led→LED→GND), not a main signal continuation. Demote to shunt
        // so parallel loads (r_led, r_load) both render as vertical drops.
        {
            for (const net of processedNets) {
                // Count all non-PG sinks
                const nonPgSinks = net.sinks.filter(s =>
                    !pgInstNames.has(s.name) && s.type !== 'entity_port'
                );
                if (nonPgSinks.length < 2) continue;

                // Find main-path sinks on this net
                const mpSinks = nonPgSinks.filter(s => mainPathNames.has(s.name));
                if (mpSinks.length === 0) continue;

                for (const s of mpSinks) {
                    const inst = instMap.get(s.name);
                    // Don't demote expansion series children
                    if (inst && inst.expansion_role === 'series') continue;

                    // Check forward depth: does this sink reach other main-path nodes?
                    const fwd = allForward.get(s.name) || [];
                    const reachesMainPath = fwd.some(n => mainPathNames.has(n) && n !== s.name);
                    if (reachesMainPath) continue;

                    // Dead-end: demote to shunt
                    mainPathNames.delete(s.name);
                    const idx = mainPathOrder.indexOf(s.name);
                    if (idx >= 0) mainPathOrder.splice(idx, 1);

                    // Find the driver (junction point) for the shunt
                    const driverName = net.driver.type === 'power_source' ? null : net.driver.name;
                    const junctionName = driverName && mainPathNames.has(driverName) ? driverName : null;

                    shuntNames.push({
                        name: s.name,
                        junctionName,
                        junctionSide: 'right'
                    });

                    // Move downstream shunts of the demoted component into shunt chains
                    for (const downName of (allForward.get(s.name) || [])) {
                        const si = shuntNames.findIndex(sh => sh.name === downName);
                        if (si >= 0) {
                            // Update to chain under the newly demoted parent
                            shuntChainDown.set(s.name, downName);
                        }
                    }
                }
            }
        }

        // (flippedNames computed after positioning in step 10b)

        // ── 7b. Reclassify: shunts connected only to off-path → branch tail ──
        // e.g., LED connected to sense (branch) should join the branch chain
        // BUT: skip items that are children in a shunt chain — they connect to
        // main path through their parent, so they must stay as shunts.
        const shuntChainChildren = new Set(shuntChainDown.values());
        for (let i = shuntNames.length - 1; i >= 0; i--) {
            const item = shuntNames[i];
            if (shuntChainChildren.has(item.name)) continue; // part of a vertical chain
            let connectsToMainOrPower = false;
            for (const net of processedNets) {
                const involved = net.sinks.some(s => s.name === item.name) || net.driver.name === item.name;
                if (!involved) continue;
                if (mainPathNames.has(net.driver.name) || net.driver.type === 'power_source') {
                    connectsToMainOrPower = true; break;
                }
                for (const s of net.sinks) {
                    if (mainPathNames.has(s.name)) { connectsToMainOrPower = true; break; }
                }
                if (connectsToMainOrPower) break;
            }
            if (!connectsToMainOrPower) {
                branchNames.push({ name: item.name });
                shuntNames.splice(i, 1);
            }
        }

        // ── 8. Topological placement for main-path nodes ──
        // Topological sort of all main-band nodes (power sources, main-path
        // instances, entity ports) then place sequentially L→R.
        // Off-path components (shunts, branches) are placed manually below
        // their junction points for the wire-down visual model.

        function getInstPorts(inst) {
            const inP = [], outP = [];
            const seen = new Set();
            for (const c of inst.connections) {
                if (gndNetNames.has(c.signal)) continue;
                const k = `${inst.name}.${c.port}`;
                const r = instPortRoles.get(k);
                if (!r) continue;
                if (r.has('in') && !seen.has(c.port + '_in')) { seen.add(c.port + '_in'); inP.push(c.port); }
                if (r.has('out') && !seen.has(c.port + '_out')) { seen.add(c.port + '_out'); outP.push(c.port); }
            }
            return { inP, outP };
        }

        const inputPorts = data.ports.filter(p => p.direction === 'in');
        const outputPorts = data.ports.filter(p => p.direction === 'out');

        // Build DAG over main-band nodes for topological ordering
        const mainBandNodes = new Set();
        for (const ps of powerSourceNodes) mainBandNodes.add(ps.id);
        for (const name of mainPathOrder) mainBandNodes.add(name);
        if (inputPorts.length > 0) mainBandNodes.add('__entity_in__');
        if (outputPorts.length > 0) mainBandNodes.add('__entity_out__');

        const mbForward = new Map();
        const mbInDegree = new Map();
        for (const n of mainBandNodes) { mbForward.set(n, []); mbInDegree.set(n, 0); }

        for (const net of processedNets) {
            const dName = net.driver.type === 'power_source' ? net.driver.name
                : net.driver.type === 'entity_port' ? '__entity_in__'
                : net.driver.name;
            if (!mainBandNodes.has(dName)) continue;
            for (const s of net.sinks) {
                const sName = s.type === 'entity_port' ? '__entity_out__' : s.name;
                if (!mainBandNodes.has(sName) || dName === sName) continue;
                mbForward.get(dName).push(sName);
                mbInDegree.set(sName, mbInDegree.get(sName) + 1);
            }
        }

        // Recover implicit edges lost during PG filtering.
        // When a regulator output drives a PG symbol (e.g., reg5.VO → V5),
        // the PG sink is removed, losing the ordering constraint between
        // the regulator and the downstream power source it feeds.
        {
            const pgToPowerSource = new Map();
            for (const net of (data.nets || [])) {
                if (net.net_class === 'ground') continue;
                if (pgInstNames.has(net.driver?.name)) {
                    const psId = `__pwr_${net.name}__`;
                    if (mainBandNodes.has(psId)) {
                        pgToPowerSource.set(net.driver.name, psId);
                    }
                }
            }
            for (const net of (data.nets || [])) {
                if (net.net_class === 'ground') continue;
                const dName = net.driver?.name;
                if (!dName || !mainBandNodes.has(dName)) continue;
                for (const s of (net.sinks || [])) {
                    const psId = pgToPowerSource.get(s.name);
                    if (psId && mainBandNodes.has(psId) && psId !== dName) {
                        mbForward.get(dName).push(psId);
                        mbInDegree.set(psId, mbInDegree.get(psId) + 1);
                    }
                }
            }
        }

        // Add explicit edges from parent → series expansion children
        // so they are ordered consecutively in the topological sort.
        // Also remove edges from power sources → series children, because
        // the series child's output *feeds* the power net (e.g. inductor
        // smooths SW to create VOUT); the power source edge would force
        // the child after the power symbol it actually supplies.
        const seriesChildNames = new Set();
        for (const [parentName, group] of expansionGroups) {
            if (!mainBandNodes.has(parentName)) continue;
            for (const childName of group.series) {
                if (!mainBandNodes.has(childName)) continue;
                seriesChildNames.add(childName);
                if (!mbForward.get(parentName).includes(childName)) {
                    mbForward.get(parentName).push(childName);
                    mbInDegree.set(childName, mbInDegree.get(childName) + 1);
                }
            }
        }
        // Remove power-source → series-child edges (they invert the real flow)
        for (const psNode of powerSourceNodes) {
            const fwd = mbForward.get(psNode.id);
            if (!fwd) continue;
            for (let i = fwd.length - 1; i >= 0; i--) {
                if (seriesChildNames.has(fwd[i])) {
                    mbInDegree.set(fwd[i], mbInDegree.get(fwd[i]) - 1);
                    fwd.splice(i, 1);
                }
            }
        }

        // Kahn's topological sort
        const mainBandOrder = [];
        const topoQueue = [];
        for (const [n, deg] of mbInDegree) { if (deg === 0) topoQueue.push(n); }
        while (topoQueue.length > 0) {
            const cur = topoQueue.shift();
            mainBandOrder.push(cur);
            for (const next of (mbForward.get(cur) || [])) {
                const newDeg = mbInDegree.get(next) - 1;
                mbInDegree.set(next, newDeg);
                if (newDeg === 0) topoQueue.push(next);
            }
        }
        // ── 8b. Detect feedback components among cycle-stuck nodes ──
        // Nodes with remaining in-degree > 0 after Kahn's sort form cycles.
        // A feedback component (e.g., R_fb from amp.OUT back to amp.INM)
        // should be taken off the main path and placed near its junction.
        const cycleStuck = new Set();
        for (const n of mainBandNodes) {
            if (!mainBandOrder.includes(n)) cycleStuck.add(n);
        }
        const feedbackNames = [];
        if (cycleStuck.size > 0) {
            // For each cycle-stuck node, count how many DISTINCT main-band
            // neighbors it connects to (both forward and backward).
            // The one with fewest distinct neighbors is the feedback element.
            const neighborCount = new Map();
            for (const n of cycleStuck) {
                const neighbors = new Set();
                for (const fwd of (mbForward.get(n) || [])) {
                    if (mainBandNodes.has(fwd)) neighbors.add(fwd);
                }
                // Also check reverse edges
                for (const [src, dsts] of mbForward) {
                    if (dsts.includes(n) && mainBandNodes.has(src)) neighbors.add(src);
                }
                neighborCount.set(n, neighbors.size);
            }
            // Demote cycle-stuck nodes that connect to only 1 other node
            // (classic feedback: r_fb connects only to amp via both edges)
            for (const n of cycleStuck) {
                if (neighborCount.get(n) <= 1) {
                    // This is a feedback element — find which node it feeds back to
                    const fwdTargets = (mbForward.get(n) || []).filter(t => mainBandNodes.has(t));
                    const junctionName = fwdTargets.length > 0 ? fwdTargets[0] : null;
                    feedbackNames.push({ name: n, junctionName, junctionSide: 'right' });
                    mainPathNames.delete(n);
                    const mpoIdx = mainPathOrder.indexOf(n);
                    if (mpoIdx >= 0) mainPathOrder.splice(mpoIdx, 1);
                } else {
                    // Keep on main path
                    mainBandOrder.push(n);
                }
            }
        }
        // Remaining cycle-stuck nodes (kept) were already added above
        // Non-cycle-stuck nodes that weren't reached (shouldn't happen) go at end
        for (const n of mainBandNodes) {
            if (!mainBandOrder.includes(n) && !feedbackNames.some(f => f.name === n)) {
                mainBandOrder.push(n);
            }
        }

        // Off-path names (computed after feedback reclassification)
        const offPathNames = new Set([
            ...shuntNames.map(s => s.name),
            ...decouplingNames.map(d => d.name),
            ...branchNames.map(b => b.name),
            ...feedbackNames.map(f => f.name)
        ]);

        // ── 9. Sequential L→R placement with adaptive gaps ──
        const positions = new Map();
        let curX = 40;

        // Pre-compute annotation text widths for adaptive gap sizing
        const sim = data.simulation || {};
        const netVoltages = sim.net_voltages || {};
        const instCurrents = sim.instance_currents || {};

        function annotationGap(netName, driverInst) {
            let maxTextW = 0;
            const v = netVoltages[netName];
            if (v != null) maxTextW = Math.max(maxTextW, measureTextWidth(formatVoltage(v), FONT_SIZE - 2));
            const a = instCurrents[driverInst];
            if (a != null) maxTextW = Math.max(maxTextW, measureTextWidth(formatCurrent(Math.abs(a)), FONT_SIZE - 2));
            return maxTextW > 0 ? maxTextW + ANNOTATION_PAD * 2 : 0;
        }

        for (const nodeId of mainBandOrder) {
            let w, h;
            const ps = powerSourceNodes.find(p => p.id === nodeId);
            if (ps) {
                w = Math.max(80, measureTextWidth(ps.label, FONT_SIZE) + 24);
                h = 28;
            } else if (nodeId === '__entity_in__') {
                h = HEADER_HEIGHT + ENTITY_PADDING * 2 + inputPorts.length * PORT_SPACING;
                let maxW = 0;
                for (const p of inputPorts) maxW = Math.max(maxW, measureTextWidth(p.name, FONT_SIZE));
                w = Math.max(ENTITY_BOX_MIN_WIDTH, maxW + 50);
            } else if (nodeId === '__entity_out__') {
                h = HEADER_HEIGHT + ENTITY_PADDING * 2 + outputPorts.length * PORT_SPACING;
                let maxW = 0;
                for (const p of outputPorts) maxW = Math.max(maxW, measureTextWidth(p.name, FONT_SIZE));
                w = Math.max(ENTITY_BOX_MIN_WIDTH, maxW + 50);
            } else {
                const inst = instMap.get(nodeId);
                if (!inst) continue;
                const { inP, outP } = getInstPorts(inst);
                const size = computeBoxSize(nodeId, inst.entity_type, inP, outP, inst.parameters, inst.category);
                w = size.w; h = size.h;
            }
            positions.set(nodeId, { x: curX, y: 40, w, h });

            // Adaptive gap: widen only when annotation text needs room
            let gap = BASE_GAP;
            const idx = mainBandOrder.indexOf(nodeId);
            if (idx + 1 < mainBandOrder.length) {
                const nextNode = mainBandOrder[idx + 1];
                for (const net of processedNets) {
                    const isDriver = net.driver.name === nodeId;
                    const isSink = net.sinks.some(s => s.name === nextNode);
                    if (isDriver && isSink) {
                        gap = Math.max(gap, annotationGap(net.name, nodeId));
                        break;
                    }
                }
            }
            curX += w + Math.min(gap, MAX_GAP);
        }

        // ── 9a. Align main-path nodes by first-port Y position ──
        // Ensure all main-band nodes have their first port at the same Y
        // so wires are perfectly horizontal through the main path.
        // Instance first-port offset: HEADER_HEIGHT + INSTANCE_PADDING + 0.5 * PORT_SPACING
        // Power source port offset: h / 2
        {
            const instPortOffset = HEADER_HEIGHT + INSTANCE_PADDING + 0.5 * PORT_SPACING;
            // Compute the maximum first-port Y across all main-band nodes
            let maxPortY = 0;
            for (const ps of powerSourceNodes) {
                const pos = positions.get(ps.id);
                if (pos) maxPortY = Math.max(maxPortY, pos.y + pos.h / 2);
            }
            for (const name of mainPathOrder) {
                const pos = positions.get(name);
                if (!pos) continue;
                const inst = instMap.get(name);
                const cat = inst ? inst.category : '';
                // Symbol categories use h/2 (port at center), box categories use header offset
                // OpAmp: first input is at h/2 - 10, so align to that
                const offset = cat === 'opamp' ? pos.h / 2 - 10
                    : isSymbolCategory(cat) ? pos.h / 2 : instPortOffset;
                maxPortY = Math.max(maxPortY, pos.y + offset);
            }
            // Reposition each node so its first port is at maxPortY
            for (const ps of powerSourceNodes) {
                const pos = positions.get(ps.id);
                if (pos) pos.y = maxPortY - pos.h / 2;
            }
            for (const name of mainPathOrder) {
                const pos = positions.get(name);
                if (!pos) continue;
                const inst = instMap.get(name);
                const cat = inst ? inst.category : '';
                const offset = cat === 'opamp' ? pos.h / 2 - 10
                    : isSymbolCategory(cat) ? pos.h / 2 : instPortOffset;
                pos.y = maxPortY - offset;
            }
        }

        // ── 10. Wire-down placement for off-path components ──

        // Find which main-path node each off-path component connects to
        function findJunction(offName, depth) {
            if ((depth || 0) > 5) return null;
            for (const net of processedNets) {
                const isSink = net.sinks.some(s => s.name === offName);
                const isDriver = net.driver.name === offName;
                if (!isSink && !isDriver) continue;
                if (mainPathNames.has(net.driver.name)) {
                    return { name: net.driver.name, side: 'right', netName: net.name };
                }
                for (const s of net.sinks) {
                    if (mainPathNames.has(s.name)) {
                        const side = (net.driver.type === 'power_source') ? 'left' : 'right';
                        return { name: s.name, side, netName: net.name };
                    }
                }
                if (net.driver.type !== 'power_source' && net.driver.name !== offName && !mainPathNames.has(net.driver.name)) {
                    const traced = findJunction(net.driver.name, (depth || 0) + 1);
                    if (traced) return traced;
                }
                for (const s of net.sinks) {
                    if (s.name !== offName && !mainPathNames.has(s.name)) {
                        const traced = findJunction(s.name, (depth || 0) + 1);
                        if (traced) return traced;
                    }
                }
            }
            return mainPathOrder.length > 0
                ? { name: mainPathOrder[0], side: 'left', netName: '' }
                : null;
        }

        for (const item of [...shuntNames, ...decouplingNames, ...branchNames]) {
            if (item.junctionName) continue; // Pre-set by parallel branch detection

            // For expansion shunt children, use the series sibling as junction.
            // Determine left/right based on which net the shunt shares with
            // the series child: input-side net → left, output-side net → right.
            const inst = instMap.get(item.name);
            if (inst && inst.expansion_parent && inst.expansion_role === 'shunt') {
                const group = expansionGroups.get(inst.expansion_parent);
                if (group && group.series.length > 0) {
                    const seriesName = group.series[0];
                    const seriesInst = instMap.get(seriesName);
                    item.junctionName = seriesName;
                    // Default to right; override to left if shunt shares the
                    // series child's input net (e.g., catch diode on SW node).
                    item.junctionSide = 'right';
                    if (seriesInst) {
                        const seriesInputNets = new Set();
                        for (const c of seriesInst.connections) {
                            if (c.pin_direction === 'in' || c.direction === 'in')
                                seriesInputNets.add(c.signal);
                        }
                        const shuntNets = inst.connections
                            .filter(c => !gndNetNames.has(c.signal))
                            .map(c => c.signal);
                        if (shuntNets.some(n => seriesInputNets.has(n))) {
                            item.junctionSide = 'left';
                        }
                    }
                    continue;
                }
            }

            const j = findJunction(item.name);
            if (j) {
                item.junctionName = j.name;
                item.junctionSide = j.side;
                item.junctionNet = j.netName;
            }
        }

        // Redirect non-expansion shunts away from series expansion children.
        // If a regular shunt/decoupling junctions at a series child (e.g., L1),
        // move it past the expansion group so it doesn't overlap visually.
        // Shunts on the output net go to the next main-path node's RIGHT side
        // (end of chain); shunts on the input net go to the parent's RIGHT side.
        {
            const seriesChildSet = new Map(); // childName → parentName
            for (const [parentName, group] of expansionGroups) {
                for (const s of group.series) seriesChildSet.set(s, parentName);
            }
            for (const item of [...shuntNames, ...decouplingNames, ...branchNames]) {
                if (!item.junctionName || !seriesChildSet.has(item.junctionName)) continue;
                const inst = instMap.get(item.name);
                if (inst && inst.expansion_parent) continue; // expansion children stay
                const seriesChild = item.junctionName;
                const parentName = seriesChildSet.get(seriesChild);
                const seriesInst = instMap.get(seriesChild);
                // Determine if shunt connects to series child's input or output net
                let onInputNet = false;
                if (seriesInst && inst) {
                    const seriesInputNets = new Set();
                    for (const c of seriesInst.connections) {
                        if (c.pin_direction === 'in' || c.direction === 'in')
                            seriesInputNets.add(c.signal);
                    }
                    const shuntNets = inst.connections
                        .filter(c => !gndNetNames.has(c.signal))
                        .map(c => c.signal);
                    onInputNet = shuntNets.some(n => seriesInputNets.has(n));
                }
                if (onInputNet && parentName) {
                    // Input-net shunt → junction at parent's right side
                    item.junctionName = parentName;
                    item.junctionSide = 'right';
                } else {
                    // Output-net shunt → junction at next main-path node's right side
                    const idx = mainPathOrder.indexOf(seriesChild);
                    if (idx >= 0 && idx + 1 < mainPathOrder.length) {
                        item.junctionName = mainPathOrder[idx + 1];
                        item.junctionSide = 'right';
                    } else {
                        // Series child is last on main path — use a virtual post-expansion
                        // junction so placement puts this shunt outside the expansion group.
                        item.junctionName = '__post_expansion_' + parentName + '__';
                        item.junctionSide = 'right';
                        item._postExpansionParent = parentName;
                    }
                }
            }
        }

        // ── 9b. Gap expansion: ensure main-path gaps are wide enough for off-path items ──
        // Pre-compute sizes of off-path items
        const offPathSizes = new Map();
        for (const item of [...shuntNames, ...decouplingNames, ...branchNames, ...feedbackNames]) {
            const inst = instMap.get(item.name);
            if (!inst) continue;
            const { inP, outP } = getInstPorts(inst);
            const sz = computeBoxSize(item.name, inst.entity_type, inP, outP, inst.parameters, inst.category);
            // Swap w/h for shunt symbol components (vertical orientation)
            // Check both the early shuntInstNames set AND the shuntNames list
            // (which includes components demoted to shunt in section 7d)
            const isShunt = shuntInstNames.has(item.name) || shuntNames.some(s => s.name === item.name);
            if (isShunt && isSymbolCategory(inst.category)) {
                offPathSizes.set(item.name, { w: sz.h, h: sz.w });
            } else {
                offPathSizes.set(item.name, sz);
            }
        }

        // Group shunts/decoupling by (junctionName, junctionSide) for gap computation
        const verticalDropItems = [...shuntNames, ...decouplingNames];
        const dropGroups = new Map();
        for (const item of verticalDropItems) {
            const key = `${item.junctionName || '__none__'}_${item.junctionSide || 'right'}`;
            if (!dropGroups.has(key)) dropGroups.set(key, { junctionName: item.junctionName, side: item.junctionSide, items: [] });
            dropGroups.get(key).items.push(item);
        }

        // Also group branches
        const branchGroups = new Map();
        for (const item of branchNames) {
            const key = `${item.junctionName || '__none__'}_${item.junctionSide || 'right'}`;
            if (!branchGroups.has(key)) branchGroups.set(key, { junctionName: item.junctionName, side: item.junctionSide, items: [] });
            branchGroups.get(key).items.push(item);
        }

        // Compute total width needed at each gap between consecutive main-path nodes
        // A "gap" is between mainPathOrder[i] and mainPathOrder[i+1].
        // Items on 'right' side of node i and 'left' side of node i+1 share this gap.
        const MIN_ITEM_GAP_BASE = 30;  // minimum gap between off-path items
        const MIN_EDGE_PAD = 30;  // minimum padding from main-path node edges

        // Compute how far a shunt item's visuals extend beyond its box edges.
        // For vertical symbol components, labels are drawn:
        //   name:  right-aligned at (el.x - 4, cy)  → extends LEFT
        //   value: left-aligned  at (el.x + w + 4, cy) → extends RIGHT (resistors)
        //          right-aligned at (el.x - 4, cy+offset) → extends LEFT (others)
        // Wire annotations on vertical segments:
        //   voltage: right-aligned at wire.x - 6  → extends LEFT
        //   current: left-aligned  at wire.x + 6  → extends RIGHT
        // Returns { left, right } overhang beyond the component box edges.
        function shuntItemOverhang(itemName) {
            let leftOverhang = 0, rightOverhang = 0;
            const inst = instMap.get(itemName);
            const sz = offPathSizes.get(itemName);
            if (!inst || !sz) return { left: 0, right: 0 };
            const isShunt = shuntInstNames.has(itemName) || shuntNames.some(s => s.name === itemName);
            const isSymbol = isShunt && isSymbolCategory(inst.category);
            if (isSymbol) {
                // Name label extends LEFT from box: textAlign 'right' at el.x - 4
                // Use the shorter of handle/refdes so layout stays compact
                const labelForLayout = inst.refdes && inst.refdes.length < itemName.length ? inst.refdes : itemName;
                const nameW = measureTextWidth(labelForLayout, FONT_SIZE - 1);
                leftOverhang = Math.max(leftOverhang, nameW + 4);
                // Value label: resistors have value rotated inside the box (no overhang).
                // Non-resistor values extend LEFT (right-aligned at el.x - 4).
                const paramStr = (inst.parameters || [])
                    .filter(p => p[1] && INLINE_PARAM_KEYS.has(p[0]))
                    .map(p => formatParamValue(p[1])).join(', ');
                if (paramStr && inst.category !== 'resistor') {
                    const valW = measureTextWidth(paramStr, FONT_SIZE - 2);
                    leftOverhang = Math.max(leftOverhang, valW + 4);
                }
            }
            // Wire annotation extents (voltage LEFT, current RIGHT of wire center)
            for (const net of processedNets) {
                const isSink = net.sinks.some(s => s.name === itemName);
                if (!isSink) continue;
                const v = netVoltages[net.name];
                if (v != null) {
                    const vw = measureTextWidth(formatVoltage(v), FONT_SIZE - 2) + 6;
                    // Voltage is at wire center (≈ box center), extends left;
                    // overhang beyond box left edge = vw - box.w/2
                    leftOverhang = Math.max(leftOverhang, vw - sz.w / 2);
                }
                const a = instCurrents[itemName];
                if (a != null) {
                    const cw = measureTextWidth(formatCurrent(Math.abs(a)), FONT_SIZE - 2) + 6;
                    rightOverhang = Math.max(rightOverhang, cw - sz.w / 2);
                }
                break;
            }
            return { left: Math.max(0, leftOverhang), right: Math.max(0, rightOverhang) };
        }

        // Max single-side overhang (for gap sizing in groupTotalWidth and placement loops)
        function shuntAnnotationWidth(itemName) {
            const ext = shuntItemOverhang(itemName);
            return Math.max(ext.left, ext.right);
        }

        // Compute total width needed for a group of off-path items
        function groupTotalWidth(items) {
            let total = 0;
            for (const item of items) {
                const sz = offPathSizes.get(item.name);
                if (sz) total += sz.w;
            }
            // Add adaptive gaps between consecutive items using directional overhangs
            if (items.length > 1) {
                for (let i = 0; i < items.length - 1; i++) {
                    const thisOH = shuntItemOverhang(items[i].name);
                    const nextOH = shuntItemOverhang(items[i + 1].name);
                    total += Math.max(MIN_ITEM_GAP_BASE, thisOH.right + nextOH.left + ANNOTATION_PAD);
                }
            }
            // Also account for first item's left overhang and last item's right overhang
            if (items.length > 0) {
                total += shuntItemOverhang(items[0].name).left;
                total += shuntItemOverhang(items[items.length - 1].name).right;
            }
            return total;
        }

        // For each pair of consecutive main-path nodes, compute space needed
        // We'll iterate through mainPathOrder plus power sources (which are left of everything)
        // mainBandOrder was computed in step 8 (topological sort)

        // For each gap, compute how much width is needed
        for (let gi = 0; gi < mainBandOrder.length - 1; gi++) {
            const leftNode = mainBandOrder[gi];
            const rightNode = mainBandOrder[gi + 1];
            const leftPos = positions.get(leftNode);
            const rightPos = positions.get(rightNode);
            if (!leftPos || !rightPos) continue;

            const currentGap = rightPos.x - (leftPos.x + leftPos.w);

            // Find all drop groups that need space in this gap
            let neededWidth = 0;

            for (const [, group] of dropGroups) {
                // 'right' side of leftNode — items hanging right of leftNode
                if (group.junctionName === leftNode && group.side === 'right') {
                    neededWidth += groupTotalWidth(group.items) + MIN_EDGE_PAD * 2;
                }
                // 'left' side of rightNode — items hanging left of rightNode
                if (group.junctionName === rightNode && group.side === 'left') {
                    neededWidth += groupTotalWidth(group.items) + MIN_EDGE_PAD * 2;
                }
            }

            // Branch chains also consume horizontal space in this gap
            for (const [, group] of branchGroups) {
                if (group.junctionName === leftNode && group.side === 'right') {
                    neededWidth += groupTotalWidth(group.items) + MIN_EDGE_PAD * 2;
                }
            }

            // If gap is too small, expand by shifting rightNode and everything after it
            if (neededWidth > currentGap) {
                const shift = neededWidth - currentGap;
                for (let si = gi + 1; si < mainBandOrder.length; si++) {
                    const pos = positions.get(mainBandOrder[si]);
                    if (pos) pos.x += shift;
                }
            }
        }

        // Also handle items on the 'left' side of the first main-path node
        // (items between power sources and first main-path component)
        if (mainBandOrder.length > 0) {
            const firstNode = mainBandOrder[0];
            for (const [, group] of dropGroups) {
                if (group.junctionName === firstNode && group.side === 'left') {
                    const needed = groupTotalWidth(group.items) + MIN_EDGE_PAD * 2;
                    // Check space from x=0 to firstNode
                    const firstPos = positions.get(firstNode);
                    if (firstPos && needed > firstPos.x - 40) {
                        const shift = needed - (firstPos.x - 40);
                        for (const name of mainBandOrder) {
                            const pos = positions.get(name);
                            if (pos) pos.x += shift;
                        }
                    }
                }
            }
        }

        // ── 10a. Compute main-path bottom edge for shunt placement ──
        let mainBandBottom = 0;
        for (const name of mainPathOrder) {
            const pos = positions.get(name);
            if (pos) mainBandBottom = Math.max(mainBandBottom, pos.y + pos.h);
        }
        for (const ps of powerSourceNodes) {
            const pos = positions.get(ps.id);
            if (pos) mainBandBottom = Math.max(mainBandBottom, pos.y + pos.h);
        }
        const SHUNT_DROP = 80;
        // Normalize all shunt heights to the tallest so wires and GND stubs align
        let maxShuntH = 0;
        for (const item of verticalDropItems) {
            const sz = offPathSizes.get(item.name);
            if (sz) maxShuntH = Math.max(maxShuntH, sz.h);
        }
        for (const item of verticalDropItems) {
            const sz = offPathSizes.get(item.name);
            if (sz) sz.h = maxShuntH;
        }
        const shuntY = mainBandBottom + SHUNT_DROP;

        // Create virtual positions for post-expansion junction points.
        // These represent a point just past the expansion group's rightmost element.
        for (const item of verticalDropItems) {
            if (!item._postExpansionParent) continue;
            const parentName = item._postExpansionParent;
            const vKey = item.junctionName; // '__post_expansion_<parent>__'
            if (positions.has(vKey)) continue;
            // Find rightmost position of any expansion group member (parent + children)
            let maxRight = 0;
            const parentPos = positions.get(parentName);
            if (parentPos) maxRight = parentPos.x + parentPos.w;
            const group = expansionGroups.get(parentName);
            if (group) {
                for (const cn of [...group.series, ...group.shunt]) {
                    const cp = positions.get(cn);
                    if (cp) maxRight = Math.max(maxRight, cp.x + cp.w);
                }
            }
            // Place virtual junction past the expansion group
            positions.set(vKey, { x: maxRight + PORT_STUB_LEN * 2, y: mainBandBottom - 20, w: 0, h: 0 });
        }

        // ── 10b. Place shunts/decoupling with width-aware distribution ──
        for (const [, group] of dropGroups) {
            const jPos = positions.get(group.junctionName);
            if (!jPos) continue;
            const items = group.items;

            // Compute total width of items in this group
            let totalItemWidth = 0;
            const itemSizes = [];
            for (const item of items) {
                const sz = offPathSizes.get(item.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                itemSizes.push(sz);
                totalItemWidth += sz.w;
            }

            const SHUNT_PORT_OFFSET = 20; // offset from port dot so T-junction is clear
            if (group.side === 'left') {
                // Left side: rightmost item offset left of input port dot, grow left
                const portDotX = jPos.x - PORT_STUB_LEN - SHUNT_PORT_OFFSET;
                let rx = portDotX + itemSizes[items.length - 1].w / 2;
                for (let i = items.length - 1; i >= 0; i--) {
                    const sz = itemSizes[i];
                    rx -= sz.w;
                    positions.set(items[i].name, { x: rx, y: shuntY, w: sz.w, h: sz.h });
                    // Gap accounts for this item's left label overhang + next item's right
                    const thisOH = shuntItemOverhang(items[i].name);
                    const nextOH = i > 0 ? shuntItemOverhang(items[i - 1].name) : { left: 0, right: 0 };
                    rx -= Math.max(MIN_ITEM_GAP_BASE, thisOH.left + nextOH.right + ANNOTATION_PAD);
                }
            } else {
                // Right side: leftmost item offset right of output port dot, grow right
                const portDotX = jPos.x + jPos.w + PORT_STUB_LEN + SHUNT_PORT_OFFSET;
                let lx = portDotX - itemSizes[0].w / 2;
                for (let i = 0; i < items.length; i++) {
                    const sz = itemSizes[i];
                    positions.set(items[i].name, { x: lx, y: shuntY, w: sz.w, h: sz.h });
                    // Gap accounts for this item's right overhang + next item's left overhang
                    const thisOH = shuntItemOverhang(items[i].name);
                    const nextOH = i + 1 < items.length ? shuntItemOverhang(items[i + 1].name) : { left: 0, right: 0 };
                    lx += sz.w + Math.max(MIN_ITEM_GAP_BASE, thisOH.right + nextOH.left + ANNOTATION_PAD);
                }
            }
        }

        // Resolve overlaps among all shunt/decoupling items
        // Sort by X and push rightward when items overlap
        {
            const allDrop = verticalDropItems.filter(i => positions.has(i.name));
            allDrop.sort((a, b) => positions.get(a.name).x - positions.get(b.name).x);
            for (let i = 1; i < allDrop.length; i++) {
                const prev = positions.get(allDrop[i - 1].name);
                const curr = positions.get(allDrop[i].name);
                // Account for labels + wire annotation overhang on both sides:
                // prev's right overhang (value label for resistors, current annotation)
                // curr's left overhang (name label, voltage annotation)
                const prevOH = shuntItemOverhang(allDrop[i - 1].name);
                const currOH = shuntItemOverhang(allDrop[i].name);
                const effectiveGap = Math.max(MIN_ITEM_GAP_BASE, prevOH.right + currOH.left + ANNOTATION_PAD);
                const minX = prev.x + prev.w + effectiveGap;
                if (curr.x < minX) curr.x = minX;
            }
        }

        // Stack shunt chain members vertically (child centered below parent)
        for (const [parent, child] of shuntChainDown) {
            const parentPos = positions.get(parent);
            if (!parentPos) continue;
            const childSz = offPathSizes.get(child) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
            // If parent is on the main path (horizontal), align child under
            // the output port so the shunt drop wire is straight vertical.
            // If parent is a shunt (vertical), center under the parent.
            const parentIsMainPath = mainPathNames.has(parent);
            let cx;
            if (parentIsMainPath) {
                // Output port is at the right edge; stub extends PORT_STUB_LEN further
                cx = parentPos.x + parentPos.w + PORT_STUB_LEN;
            } else {
                cx = parentPos.x + parentPos.w / 2;
            }
            // Use common shuntY for main-path children so all shunts align vertically.
            // For shunt-under-shunt chains, stack below the parent.
            const childY = parentIsMainPath ? shuntY
                : parentPos.y + parentPos.h + PORT_STUB_LEN * 2 + 10;
            positions.set(child, {
                x: cx - childSz.w / 2,
                y: childY,
                w: childSz.w, h: childSz.h
            });
        }

        // ── 10b-post. Align all shunt column bottoms to the same Y ──
        // Find the lowest bottom Y across all shunt columns (including chain children),
        // then store a common gndTargetY on each shunt element so GND stubs align.
        {
            // Build column bottom map: for each shunt root, find the lowest child bottom
            const shuntChainParents = new Set(shuntChainDown.keys());
            const shuntChainChildSet = new Set(shuntChainDown.values());
            let maxBottomY = 0;
            for (const item of verticalDropItems) {
                const pos = positions.get(item.name);
                if (!pos) continue;
                let bottomY = pos.y + pos.h;
                // If this item has a chain child, the child's bottom is the column bottom
                if (shuntChainDown.has(item.name)) {
                    const childPos = positions.get(shuntChainDown.get(item.name));
                    if (childPos) bottomY = childPos.y + childPos.h;
                }
                // If this item IS a chain child, its bottom is the column bottom
                if (shuntChainChildSet.has(item.name)) {
                    bottomY = pos.y + pos.h;
                }
                maxBottomY = Math.max(maxBottomY, bottomY);
            }
            // Store gndTargetY on positions so GND stubs extend to common bottom
            if (maxBottomY > 0) {
                for (const item of verticalDropItems) {
                    const pos = positions.get(item.name);
                    if (pos) pos.gndTargetY = maxBottomY;
                    // Also set on chain children
                    if (shuntChainDown.has(item.name)) {
                        const childPos = positions.get(shuntChainDown.get(item.name));
                        if (childPos) childPos.gndTargetY = maxBottomY;
                    }
                }
            }
        }

        // ── 10c. Place branches as horizontal chains ──
        for (const [, group] of branchGroups) {
            const jPos = positions.get(group.junctionName);
            if (!jPos) continue;
            const ordered = orderBranchChain(group.items, processedNets);
            const isParallel = group.items.some(item => item.isParallel);

            let bx, by;
            if (isParallel) {
                // Parallel branch: start at junction's X (aligned with main-path sibling)
                bx = jPos.x;
                // Place below any shunts at this junction
                let maxYBelow = shuntY;
                for (const dItem of verticalDropItems) {
                    if (dItem.junctionName === group.junctionName) {
                        const dPos = positions.get(dItem.name);
                        if (dPos) maxYBelow = Math.max(maxYBelow, dPos.y + dPos.h);
                    }
                }
                by = maxYBelow + 60;
            } else if (group.side === 'left') {
                bx = jPos.x - 200;
                by = shuntY;
            } else {
                bx = jPos.x + jPos.w + 20;
                for (const dItem of verticalDropItems) {
                    if (dItem.junctionName === group.junctionName && dItem.junctionSide === group.side) {
                        const dPos = positions.get(dItem.name);
                        if (dPos) bx = Math.max(bx, dPos.x + dPos.w + 30);
                    }
                }
                by = shuntY;
            }

            if (isParallel) {
                // Parallel sub-chains: each head (from parallelBranchJunctions) starts a new row
                // at the same X, stacked vertically. Shunt tails drop below their head.
                let chainX = bx;
                let chainY = by;
                let prevPos = null;
                let maxBottom = by;

                for (const item of ordered) {
                    const sz = offPathSizes.get(item.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                    const isHead = parallelBranchJunctions.has(item.name);

                    if (isHead) {
                        // New sub-chain row
                        if (prevPos) chainY = maxBottom + 60;
                        chainX = bx;
                        positions.set(item.name, { x: chainX, y: chainY, w: sz.w, h: sz.h });
                        prevPos = { x: chainX, y: chainY, w: sz.w, h: sz.h };
                        maxBottom = Math.max(maxBottom, chainY + sz.h);
                        chainX += sz.w + 120;
                    } else if (shuntInstNames.has(item.name) && prevPos) {
                        const portDotX = prevPos.x + prevPos.w + PORT_STUB_LEN + 20;
                        const dropX = portDotX - sz.w / 2;
                        const dropY = prevPos.y + prevPos.h + SHUNT_DROP;
                        positions.set(item.name, { x: dropX, y: dropY, w: sz.w, h: sz.h });
                        maxBottom = Math.max(maxBottom, dropY + sz.h);
                    } else {
                        positions.set(item.name, { x: chainX, y: chainY, w: sz.w, h: sz.h });
                        prevPos = { x: chainX, y: chainY, w: sz.w, h: sz.h };
                        maxBottom = Math.max(maxBottom, chainY + sz.h);
                        chainX += sz.w + 120;
                    }
                }
            } else {
                let prevBranchPos = null;
                for (const item of ordered) {
                    const sz = offPathSizes.get(item.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                    if (shuntInstNames.has(item.name) && prevBranchPos) {
                        const portDotX = prevBranchPos.x + prevBranchPos.w + PORT_STUB_LEN + 20;
                        const dropX = portDotX - sz.w / 2;
                        const dropY = prevBranchPos.y + prevBranchPos.h + SHUNT_DROP;
                        positions.set(item.name, { x: dropX, y: dropY, w: sz.w, h: sz.h });
                    } else {
                        positions.set(item.name, { x: bx, y: by, w: sz.w, h: sz.h });
                        prevBranchPos = { x: bx, y: by, w: sz.w, h: sz.h };
                        bx += sz.w + 120;
                    }
                }
            }
        }

        // ── 10d-fb. Place feedback components below their junction ──
        // Feedback resistors (e.g., R_fb from op-amp output to inverting input)
        // are placed below the junction node.  Placing below keeps them out of
        // the main-path horizontal wire corridor and minimises wire crossings
        // (short L-routes from junction ports).
        // Shift left if needed to avoid overlapping the shunt column.
        const FEEDBACK_DROP = 40;
        const feedbackNameSet = new Set(feedbackNames.map(f => f.name));
        for (const fb of feedbackNames) {
            const jPos = positions.get(fb.junctionName);
            if (!jPos) continue;
            const sz = offPathSizes.get(fb.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
            const FB_GAP = 20;
            // Default: center on junction
            let fbX = jPos.x + jPos.w / 2 - sz.w / 2;
            // If shunts/decoupling sit to the right of the junction center,
            // pull feedback left so its right edge clears the shunt column.
            let minShuntX = Infinity;
            for (const item of [...shuntNames, ...decouplingNames]) {
                const sp = positions.get(item.name);
                if (sp && sp.x > jPos.x + jPos.w / 2) {
                    minShuntX = Math.min(minShuntX, sp.x);
                }
            }
            if (minShuntX < Infinity) {
                fbX = Math.min(fbX, minShuntX - FB_GAP - sz.w);
            }
            let fbY = jPos.y + jPos.h + FEEDBACK_DROP;
            // Scan down past collisions with already-placed components
            let settled = false;
            while (!settled) {
                settled = true;
                for (const [n, p] of positions) {
                    if (n === fb.name) continue;
                    if (fbX < p.x + p.w + FB_GAP && fbX + sz.w + FB_GAP > p.x &&
                        fbY < p.y + p.h + FB_GAP && fbY + sz.h + FB_GAP > p.y) {
                        fbY = p.y + p.h + FB_GAP;
                        settled = false;
                        break;
                    }
                }
            }
            positions.set(fb.name, { x: fbX, y: fbY, w: sz.w, h: sz.h });
        }

        // ── 10b. Detect flipped instances (wire direction opposite to port side) ──
        // If a component's input source is to its right, or its output sink is
        // to its left, the normal L→R port assignment produces wires that cross
        // through the component box. Flip ports so wires route cleanly.
        const flippedNames = new Set();
        for (const [name, inst] of instMap) {
            const pos = positions.get(name);
            if (!pos) continue;
            if (shuntInstNames.has(name)) continue; // shunts use NORTH port, not L/R
            const cx = pos.x + pos.w / 2;
            let backwardIn = 0, forwardIn = 0;
            let backwardOut = 0, forwardOut = 0;
            for (const c of inst.connections) {
                if (gndNetNames.has(c.signal)) continue;
                const pinType = getPortPinType(name, c.port);
                if (pinType === 'power' || pinType === 'ground') continue;
                const k = `${name}.${c.port}`;
                const r = instPortRoles.get(k);
                if (!r) continue;
                // Find the connected peer's position
                for (const net of processedNets) {
                    if (r.has('in')) {
                        // This port is an input — check where the driver is
                        const isSink = net.sinks.some(s => s.name === name && s.port === c.port);
                        if (isSink && net.driver.name) {
                            const driverPos = positions.get(net.driver.name);
                            if (driverPos) {
                                const driverCx = driverPos.x + driverPos.w / 2;
                                if (driverCx > cx) backwardIn++; else forwardIn++;
                            }
                        }
                    }
                    if (r.has('out')) {
                        // This port is an output — check where sinks are
                        if (net.driver.name === name && net.driver.port === c.port) {
                            for (const s of net.sinks) {
                                const sinkPos = positions.get(s.name);
                                if (sinkPos) {
                                    const sinkCx = sinkPos.x + sinkPos.w / 2;
                                    if (sinkCx < cx) backwardOut++; else forwardOut++;
                                }
                            }
                        }
                    }
                }
            }
            const backward = backwardIn + backwardOut;
            const forward = forwardIn + forwardOut;
            if (backward > 0 && backward >= forward) {
                flippedNames.add(name);
            }
        }
        // Force-flip feedback components: their input comes from the junction's
        // output (right) and their output goes to the junction's input (left).
        for (const fb of feedbackNames) flippedNames.add(fb.name);

        // ── 10c. Reposition flipped components so wires don't cross through blocks ──
        // A flipped component has its input on the RIGHT. Position it so that right
        // edge aligns with where the input wire comes from (the driver's output side),
        // preventing wires from crossing intermediate blocks.
        for (const name of flippedNames) {
            // Feedback components are already centered above their junction (step 10d-fb).
            // Repositioning them here would break the vertical alignment needed for
            // clean up/down wires between feedback and junction.
            if (feedbackNames.some(f => f.name === name)) continue;
            const pos = positions.get(name);
            if (!pos) continue;
            const inst = instMap.get(name);
            if (!inst) continue;
            // Find the output destination's left edge (where the return wire goes)
            let sinkLeftX = null;
            for (const c of inst.connections) {
                if (gndNetNames.has(c.signal)) continue;
                const pinType = getPortPinType(name, c.port);
                if (pinType === 'power' || pinType === 'ground') continue;
                const k = `${name}.${c.port}`;
                const r = instPortRoles.get(k);
                if (!r || !r.has('out')) continue;
                for (const net of processedNets) {
                    if (net.driver.name === name && net.driver.port === c.port) {
                        for (const s of net.sinks) {
                            const sinkPos = positions.get(s.name);
                            if (sinkPos && (sinkLeftX === null || sinkPos.x < sinkLeftX)) {
                                sinkLeftX = sinkPos.x;
                            }
                        }
                    }
                }
            }
            if (sinkLeftX !== null) {
                // Offset left so the output stub dot clears the destination's input port stubs.
                // Output dot will be at newX - PORT_STUB_LEN, routed up then right to
                // destination's input dot at sinkLeftX - PORT_STUB_LEN.
                const newX = sinkLeftX - PORT_STUB_LEN - 10;
                positions.set(name, { x: newX, y: pos.y, w: pos.w, h: pos.h });
            }
        }

        // ── 10d. Resolve overlapping components ──
        // After all positioning (including flip repositioning), ensure no two
        // components overlap. Shift colliding off-path components down.
        {
            const GAP = 20; // minimum spacing between component bounding boxes
            const allNames = [...positions.keys()].filter(n => n !== '__entity_in__' && n !== '__entity_out__');
            // Iterate off-path names (excluding feedback, already scan-placed);
            // nudge them if they collide with anything
            const offPath = allNames.filter(n => offPathNames.has(n) && !feedbackNameSet.has(n));
            for (const name of offPath) {
                let pos = positions.get(name);
                let moved = true;
                while (moved) {
                    moved = false;
                    for (const other of allNames) {
                        if (other === name) continue;
                        const op = positions.get(other);
                        if (!op) continue;
                        // Check bounding box overlap with gap
                        const overlapX = pos.x < op.x + op.w + GAP && pos.x + pos.w + GAP > op.x;
                        const overlapY = pos.y < op.y + op.h + GAP && pos.y + pos.h + GAP > op.y;
                        if (overlapX && overlapY) {
                            // Shift down below the colliding component
                            const newY = op.y + op.h + GAP;
                            pos = { x: pos.x, y: newY, w: pos.w, h: pos.h };
                            positions.set(name, pos);
                            moved = true;
                            break; // re-check all after shift
                        }
                    }
                }
            }
        }

        // ── 11. Build layout elements ──

        // Entity input
        if (inputPorts.length > 0 && positions.has('__entity_in__')) {
            const pos = positions.get('__entity_in__');
            const outP = inputPorts.map((p, i) => ({
                name: p.name, x: pos.x + pos.w, y: pos.y + HEADER_HEIGHT + ENTITY_PADDING + (i + 0.5) * PORT_SPACING,
                isClock: p.type === 'clock', isReset: p.type === 'reset'
            }));
            layoutElements.push({ x: pos.x, y: pos.y, w: pos.w, h: pos.h, name: data.entity_name, type: 'entity_in', inputPorts: [], outputPorts: outP, gndStubs: [], pgStubs: [], line: data.entity_line });
        }

        // Power sources
        for (const ps of powerSourceNodes) {
            const pos = positions.get(ps.id);
            if (!pos) continue;
            layoutElements.push({ x: pos.x, y: pos.y, w: pos.w, h: pos.h, name: ps.id, type: 'power_source', label: ps.label, inputPorts: [], outputPorts: [{ name: 'out', x: pos.x + pos.w, y: pos.y + pos.h / 2 }], gndStubs: [], pgStubs: [] });
        }

        // Instances
        for (const [name, inst] of instMap) {
            const pos = positions.get(name);
            if (!pos) continue;
            const isFeedback = feedbackNames.some(f => f.name === name);
            const isShuntLike = !isFeedback && offPathNames.has(name) && (shuntInstNames.has(name) || !branchNames.some(b => b.name === name));

            const instInPorts = [], instOutPorts = [];
            const seenIn = new Set(), seenOut = new Set();
            const isSymbol = isSymbolCategory(inst.category);
            for (const c of inst.connections) {
                if (gndNetNames.has(c.signal)) continue;
                const k = `${name}.${c.port}`;
                const r = instPortRoles.get(k);
                if (!r) continue;

                if (isShuntLike) {
                    // Signal port on NORTH (top center) — wire drops down from main path
                    if (r.has('in') && !seenIn.has(c.port)) {
                        seenIn.add(c.port);
                        instInPorts.push({ name: c.port, x: pos.x + pos.w / 2, y: pos.y, pinType: getPortPinType(name, c.port) });
                    }
                    // Chain shunt: output port on SOUTH (bottom center) — wire continues down
                    if (shuntChainDown.has(name) && r.has('out') && !seenOut.has(c.port)) {
                        seenOut.add(c.port);
                        instOutPorts.push({ name: c.port, x: pos.x + pos.w / 2, y: pos.y + pos.h, pinType: getPortPinType(name, c.port) });
                    }
                } else if (isSymbol && inst.category !== 'opamp') {
                    // 2-pin symbol: input at left-center, output at right-center
                    const isFlipped = flippedNames.has(name);
                    const cy = pos.y + pos.h / 2;
                    if (r.has('in') && !seenIn.has(c.port)) {
                        seenIn.add(c.port);
                        const px = isFlipped ? pos.x + pos.w : pos.x;
                        instInPorts.push({ name: c.port, x: px, y: cy, pinType: getPortPinType(name, c.port), isClock: clockSignals.has(c.signal), isReset: resetSignals.has(c.signal) });
                    }
                    if (r.has('out') && !seenOut.has(c.port)) {
                        seenOut.add(c.port);
                        const px = isFlipped ? pos.x : pos.x + pos.w;
                        instOutPorts.push({ name: c.port, x: px, y: cy, pinType: getPortPinType(name, c.port) });
                    }
                } else if (isSymbol && inst.category === 'opamp') {
                    // OpAmp: INP/INM stacked on left, OUT on right-center
                    const isFlipped = flippedNames.has(name);
                    if (r.has('in') && !seenIn.has(c.port)) {
                        seenIn.add(c.port);
                        const pinType = getPortPinType(name, c.port);
                        if (pinType === 'power' || pinType === 'ground') continue; // skip VCC/GND
                        const py = pos.y + pos.h / 2 + (instInPorts.length === 0 ? -10 : 10);
                        const px = isFlipped ? pos.x + pos.w : pos.x;
                        instInPorts.push({ name: c.port, x: px, y: py, pinType, isClock: clockSignals.has(c.signal), isReset: resetSignals.has(c.signal) });
                    }
                    if (r.has('out') && !seenOut.has(c.port)) {
                        seenOut.add(c.port);
                        const pinType = getPortPinType(name, c.port);
                        if (pinType === 'power' || pinType === 'ground') continue;
                        const py = pos.y + pos.h / 2;
                        const px = isFlipped ? pos.x : pos.x + pos.w;
                        instOutPorts.push({ name: c.port, x: px, y: py, pinType });
                    }
                } else {
                    const isFlipped = flippedNames.has(name);
                    if (r.has('in') && !seenIn.has(c.port)) {
                        seenIn.add(c.port);
                        const py = pos.y + HEADER_HEIGHT + INSTANCE_PADDING + (instInPorts.length + 0.5) * PORT_SPACING;
                        const px = isFlipped ? pos.x + pos.w : pos.x;
                        instInPorts.push({ name: c.port, x: px, y: py, pinType: getPortPinType(name, c.port), isClock: clockSignals.has(c.signal), isReset: resetSignals.has(c.signal) });
                    }
                    if (r.has('out') && !seenOut.has(c.port)) {
                        seenOut.add(c.port);
                        const py = pos.y + HEADER_HEIGHT + INSTANCE_PADDING + (instOutPorts.length + 0.5) * PORT_SPACING;
                        const px = isFlipped ? pos.x : pos.x + pos.w;
                        instOutPorts.push({ name: c.port, x: px, y: py, pinType: getPortPinType(name, c.port) });
                    }
                }
            }
            // Attach simulation annotations (current, power) if available.
            // build_simulation_annotations() unifies decomposed branches (e.g. regulators)
            // into a single entry per instance, so direct lookup is sufficient.
            const simCurrent = data.simulation?.instance_currents?.[name];
            const simPowerW = data.simulation?.instance_power?.[name];
            const refdes = inst.refdes || null;
            const displayName = refdes && refdesChk?.checked ? refdes : name;
            layoutElements.push({ x: pos.x, y: pos.y, w: pos.w, h: pos.h, name, refdes, displayName, type: 'instance', entityType: inst.entity_type, parameters: inst.parameters, category: inst.category, isShunt: isShuntLike, isFlipped: flippedNames.has(name), inputPorts: instInPorts, outputPorts: instOutPorts, gndStubs: gndStubsByInst.get(name) || [], pwrStubs: pwrStubsByInst.get(name) || [], pgStubs: [], line: inst.line, simCurrent, simPower: simPowerW, gndTargetY: pos.gndTargetY });
        }

        // Entity output
        if (outputPorts.length > 0 && positions.has('__entity_out__')) {
            const pos = positions.get('__entity_out__');
            const inP = outputPorts.map((p, i) => ({
                name: p.name, x: pos.x, y: pos.y + HEADER_HEIGHT + ENTITY_PADDING + (i + 0.5) * PORT_SPACING
            }));
            layoutElements.push({ x: pos.x, y: pos.y, w: pos.w, h: pos.h, name: data.entity_name, type: 'entity_out', inputPorts: inP, outputPorts: [], gndStubs: [], pgStubs: [], line: data.entity_line });
        }

        // Populate debug panel (visible when "Debug" checkbox is checked)
        if (debugPanel) {
            const dbg = [];
            dbg.push('=== PROCESSED NETS ===');
            for (const net of processedNets) {
                dbg.push(`  ${net.name}: driver=${net.driver.name}.${net.driver.port} (type=${net.driver.type||'inst'}) sinks=[${net.sinks.map(s=>s.name+'.'+s.port).join(', ')}]`);
            }
            dbg.push('=== PG INSTANCES ===');
            dbg.push(`  ${[...pgInstNames].join(', ')}`);
            dbg.push('=== SHUNT INST NAMES (section 3) ===');
            dbg.push(`  ${[...shuntInstNames].join(', ')}`);
            dbg.push('=== CLASSIFICATION ===');
            dbg.push(`  mainPathNames: [${[...mainPathNames].join(', ')}]`);
            dbg.push(`  shuntNames: [${shuntNames.map(s=>s.name).join(', ')}]`);
            dbg.push(`  decouplingNames: [${decouplingNames.map(d=>d.name).join(', ')}]`);
            dbg.push(`  branchNames: [${branchNames.map(b=>b.name).join(', ')}]`);
            dbg.push(`  offPathNames: [${[...offPathNames].join(', ')}]`);
            dbg.push('=== MAIN BAND ORDER ===');
            dbg.push(`  [${mainBandOrder.join(', ')}]`);
            dbg.push('=== POSITIONS ===');
            for (const [name, pos] of positions) {
                dbg.push(`  ${name}: x=${pos.x.toFixed(0)} y=${pos.y.toFixed(0)} w=${pos.w.toFixed(0)} h=${pos.h.toFixed(0)}`);
            }
            dbg.push('=== INST PORT ROLES ===');
            for (const [k, v] of instPortRoles) {
                dbg.push(`  ${k}: {${[...v].join(',')}}`);
            }
            dbg.push('=== LAYOUT ELEMENTS ===');
            for (const el of layoutElements) {
                if (el.type !== 'instance') continue;
                dbg.push(`  ${el.name}: x=${el.x.toFixed(0)} y=${el.y.toFixed(0)} w=${el.w.toFixed(0)} h=${el.h.toFixed(0)} isShunt=${el.isShunt} isFlipped=${el.isFlipped} cat=${el.category}`);
                dbg.push(`    inPorts: [${el.inputPorts.map(p=>p.name+'@'+p.x.toFixed(0)+','+p.y.toFixed(0)).join('; ')}]`);
                dbg.push(`    outPorts: [${el.outputPorts.map(p=>p.name+'@'+p.x.toFixed(0)+','+p.y.toFixed(0)).join('; ')}]`);
            }
            dbg.push('=== EXPANSION GROUPS ===');
            for (const [pname, group] of expansionGroups) {
                dbg.push(`  ${pname}: series=[${group.series.join(',')}] shunt=[${group.shunt.join(',')}]`);
            }
            dbg.push('=== SHUNT JUNCTIONS ===');
            for (const item of [...shuntNames, ...decouplingNames]) {
                dbg.push(`  ${item.name}: junction=${item.junctionName} side=${item.junctionSide}`);
            }
            debugPanel.textContent = dbg.join('\n');
        }

        // ── 12. Wire routing (custom L-route / Z-route) ──
        const elByName = new Map();
        for (const el of layoutElements) elByName.set(el.name, el);

        function findPort(elName, portName, side) {
            const el = elByName.get(elName);
            if (!el) return null;
            // Search expected list first, then fallback
            const lists = side === 'out'
                ? [el.outputPorts, el.inputPorts]
                : [el.inputPorts, el.outputPorts];
            const sides = side === 'out' ? ['out', 'in'] : ['in', 'out'];
            for (let li = 0; li < lists.length; li++) {
                const p = lists[li].find(p => p.name === portName);
                if (!p) continue;
                // Return the wire connection point (at the port dot, end of stub)
                if (el.isShunt && sides[li] === 'in') {
                    // NORTH port: dot is above the box
                    return { x: p.x, y: p.y - PORT_STUB_LEN };
                }
                if (el.isShunt && sides[li] === 'out') {
                    // SOUTH port: dot is below the box (chain shunt)
                    return { x: p.x, y: p.y + PORT_STUB_LEN };
                }
                let dx;
                if (el.isFlipped) {
                    dx = sides[li] === 'out' ? -PORT_STUB_LEN : PORT_STUB_LEN;
                } else {
                    dx = sides[li] === 'out' ? PORT_STUB_LEN : -PORT_STUB_LEN;
                }
                // dir: the horizontal direction the wire should approach/depart from this dot
                // +1 = wire extends to the right, -1 = wire extends to the left
                const dir = dx > 0 ? 1 : -1;
                return { x: p.x + dx, y: p.y, dir };
            }
            return null;
        }

        const junctionPoints = [];

        // Obstacle avoidance: find a clear X for a vertical wire segment
        // that doesn't intersect any component bounding box.
        // direction: +1 = push right, -1 = push left
        const WIRE_CLEARANCE = 8;
        function clearVerticalX(startX, yMin, yMax, direction, excludeNames) {
            let x = startX;
            const excl = new Set(excludeNames || []);
            for (let iter = 0; iter < 20; iter++) {
                let blocked = false;
                for (const el of layoutElements) {
                    if (excl.has(el.name)) continue;
                    if (el.type === 'power_source') continue;
                    // Check if vertical line at x overlaps element's bounding box
                    const elLeft = el.x - WIRE_CLEARANCE;
                    const elRight = el.x + el.w + WIRE_CLEARANCE;
                    const elTop = el.y - WIRE_CLEARANCE;
                    const elBottom = el.y + el.h + WIRE_CLEARANCE;
                    if (x > elLeft && x < elRight && yMax > elTop && yMin < elBottom) {
                        // Push x outside the element
                        x = direction > 0 ? elRight : elLeft;
                        blocked = true;
                        break;
                    }
                }
                if (!blocked) break;
            }
            return x;
        }

        // Build shunt junction lookup for wire routing: shunt name → {junctionName, junctionSide}
        const shuntJunctionLookup = new Map();
        for (const item of [...shuntNames, ...decouplingNames]) {
            if (item.junctionName) {
                shuntJunctionLookup.set(item.name, { junctionName: item.junctionName, junctionSide: item.junctionSide });
            }
        }

        for (const net of processedNets) {
            const driverElName = net.driver.type === 'power_source' ? net.driver.name
                : net.driver.type === 'entity_port' ? '__entity_in__'
                : net.driver.name;
            const fromPos = findPort(driverElName, net.driver.port, 'out');
            if (!fromPos) continue;

            for (const sink of net.sinks) {
                const sinkElName = sink.type === 'entity_port' ? '__entity_out__' : sink.name;
                const toPos = findPort(sinkElName, sink.port, 'in');
                if (!toPos) continue;

                const sinkEl = elByName.get(sinkElName);
                const isShuntWire = sinkEl && sinkEl.isShunt;
                const segments = [];

                // dir: +1 = wire extends right from dot, -1 = wire extends left
                const fromDir = fromPos.dir || 1;   // driver output default: rightward
                const toDir = toPos.dir || -1;       // sink input default: leftward

                // For shunt wires, use the junction point on the main path as the
                // origin for the vertical drop, rather than routing from the driver.
                // This prevents wires from cutting through intermediate components.
                let shuntFromPos = fromPos;
                if (isShuntWire) {
                    const jInfo = shuntJunctionLookup.get(sinkElName);
                    if (jInfo) {
                        const jEl = elByName.get(jInfo.junctionName);
                        if (jEl) {
                            const jx = jInfo.junctionSide === 'right'
                                ? jEl.x + jEl.w + PORT_STUB_LEN
                                : jEl.x - PORT_STUB_LEN;
                            const jy = jEl.y + jEl.h / 2;
                            // Only override when the shunt is past the junction
                            // element's far edge — meaning the default wire from
                            // driver would cut through intermediate main-path
                            // components to reach the shunt.
                            const jElFarEdge = jEl.x + jEl.w;
                            if (jx > fromPos.x && toPos.x > jElFarEdge) {
                                shuntFromPos = { x: jx, y: fromPos.y, dir: 1 };
                            }
                        }
                    }
                }

                if (isShuntWire || (toPos.y > fromPos.y + 20 && toDir <= 0)) {

                    if (toPos.x >= shuntFromPos.x - 2) {
                        // Normal L-route: horizontal from junction to shunt X, then vertical drop
                        const jx = toPos.x;
                        const jy = shuntFromPos.y;
                        junctionPoints.push({ x: jx, y: jy });
                        if (Math.abs(shuntFromPos.x - jx) > 2) {
                            segments.push({ x1: shuntFromPos.x, y1: shuntFromPos.y, x2: jx, y2: jy });
                        }
                        segments.push({ x1: jx, y1: jy, x2: toPos.x, y2: toPos.y });
                    } else {
                        // Reverse L-route: shunt port is left of the driver port.
                        // Drop down from driver stub, then horizontal back to shunt.
                        // Use clearVerticalX to avoid cutting through components.
                        const yMin = Math.min(fromPos.y, toPos.y);
                        const yMax = Math.max(fromPos.y, toPos.y);
                        const vx = clearVerticalX(fromPos.x, yMin, yMax, +1, [driverElName, sinkElName]);
                        junctionPoints.push({ x: vx, y: fromPos.y });
                        if (Math.abs(fromPos.x - vx) > 2) {
                            segments.push({ x1: fromPos.x, y1: fromPos.y, x2: vx, y2: fromPos.y });
                        }
                        segments.push({ x1: vx, y1: fromPos.y, x2: vx, y2: toPos.y });
                        if (Math.abs(vx - toPos.x) > 2) {
                            segments.push({ x1: vx, y1: toPos.y, x2: toPos.x, y2: toPos.y });
                        }
                    }
                } else if (toDir > 0 && toPos.y > fromPos.y + 20) {
                    // Flipped sink below driver: sink expects wire from the RIGHT.
                    // Route vertical to the right, avoiding any component bounding boxes.
                    const baseX = Math.max(fromPos.x, toPos.x);
                    const yMin = Math.min(fromPos.y, toPos.y);
                    const yMax = Math.max(fromPos.y, toPos.y);
                    const routeX = clearVerticalX(baseX, yMin, yMax, +1, [driverElName, sinkElName]);
                    junctionPoints.push({ x: routeX, y: fromPos.y });
                    if (Math.abs(fromPos.x - routeX) > 2) {
                        segments.push({ x1: fromPos.x, y1: fromPos.y, x2: routeX, y2: fromPos.y });
                    }
                    segments.push({ x1: routeX, y1: fromPos.y, x2: routeX, y2: toPos.y });
                    if (Math.abs(routeX - toPos.x) > 2) {
                        segments.push({ x1: routeX, y1: toPos.y, x2: toPos.x, y2: toPos.y });
                    }
                } else if (fromDir < 0 && toPos.y < fromPos.y - 20) {
                    // Flipped driver below sink: driver departs LEFT, sink is above.
                    // Route vertical to the left, avoiding any component bounding boxes.
                    const baseX = Math.min(fromPos.x, toPos.x);
                    const yMin = Math.min(fromPos.y, toPos.y);
                    const yMax = Math.max(fromPos.y, toPos.y);
                    const routeX = clearVerticalX(baseX, yMin, yMax, -1, [driverElName, sinkElName]);
                    if (Math.abs(fromPos.x - routeX) > 2) {
                        segments.push({ x1: fromPos.x, y1: fromPos.y, x2: routeX, y2: fromPos.y });
                    }
                    segments.push({ x1: routeX, y1: fromPos.y, x2: routeX, y2: toPos.y });
                    if (Math.abs(routeX - toPos.x) > 2) {
                        segments.push({ x1: routeX, y1: toPos.y, x2: toPos.x, y2: toPos.y });
                    }
                } else if (Math.abs(fromPos.y - toPos.y) < 2) {

                    // Horizontal
                    segments.push({ x1: fromPos.x, y1: fromPos.y, x2: toPos.x, y2: toPos.y });
                } else {

                    // Z-route: horizontal → vertical → horizontal
                    const midX = (fromPos.x + toPos.x) / 2;
                    segments.push({ x1: fromPos.x, y1: fromPos.y, x2: midX, y2: fromPos.y });
                    segments.push({ x1: midX, y1: fromPos.y, x2: midX, y2: toPos.y });
                    segments.push({ x1: midX, y1: toPos.y, x2: toPos.x, y2: toPos.y });
                }

                // Classify wire as power or signal.
                // Prefer GLACIER DC simulation data (ground-truth current flow)
                // over heuristic driver-pin-type check.
                const simPower = data.simulation?.power_nets;
                let isPowerNet;
                if (simPower) {
                    // Simulation available: a net is power if GLACIER says so,
                    // or if the driver is a power source symbol (always power)
                    isPowerNet = (simPower instanceof Set ? simPower.has(net.name) : !!simPower[net.name])
                        || net.driver.type === 'power_source';
                } else {
                    // Fallback: driver pin_type heuristic
                    const driverPinType = net.driver.type === 'power_source' ? 'power'
                        : getPortPinType(net.driver.name, net.driver.port);
                    isPowerNet = driverPinType === 'power' || net.net_class === 'power';
                }

                // Gather simulation annotations for this wire
                const voltage = data.simulation?.net_voltages?.[net.name];
                // Current: use the sink component's current (current flowing into it through this wire).
                // build_simulation_annotations() unifies decomposed branches into a single entry
                // per instance, so direct lookup is sufficient.
                const current = data.simulation?.instance_currents?.[sink.name];
                // Driver current: total current leaving the driver into this net.
                // Used for the shared horizontal trunk segment on fan-out nets.
                const driverCurrent = data.simulation?.instance_currents?.[driverElName];
                const driverIsPowerSource = net.driver.type === 'power_source';
                layoutWires.push({ from: fromPos, to: toPos, sinkElName, segments, width: net.width || 1, netName: net.name, netClass: net.net_class || 'signal', isPower: isPowerNet, voltage, current, driverCurrent, driverIsPowerSource });
            }
        }
        layoutElements._junctionPoints = junctionPoints;
    }

    /** Order branch items by following net connections (graph chain) */
    function orderBranchChain(items, processedNets) {
        if (items.length <= 1) return items;
        const nameSet = new Set(items.map(i => i.name));
        const ordered = [];
        const visited = new Set();
        let start = items[0];
        for (const net of processedNets) {
            for (const s of net.sinks) {
                if (nameSet.has(s.name) && !nameSet.has(net.driver.name)) { start = items.find(i => i.name === s.name) || start; break; }
            }
        }
        let cur = start;
        while (cur && !visited.has(cur.name)) {
            visited.add(cur.name);
            ordered.push(cur);
            let next = null;
            for (const net of processedNets) {
                if (net.driver.name === cur.name) {
                    for (const s of net.sinks) {
                        if (nameSet.has(s.name) && !visited.has(s.name)) { next = items.find(i => i.name === s.name); break; }
                    }
                }
                if (next) break;
            }
            cur = next;
        }
        for (const item of items) { if (!visited.has(item.name)) ordered.push(item); }
        return ordered;
    }

    // ─────────── RENDERING ───────────

    /** Draw dashed-border group outlines around virtual-pin expansion groups */
    function drawExpansionGroups() {
        if (!schematicData) return;
        const PAD = 12;
        const LABEL_H = 14;
        // Collect groups: parent → [element bounding boxes]
        const groups = new Map();
        for (const el of layoutElements) {
            if (el.type !== 'instance') continue;
            const inst = schematicData.instances.find(i => i.name === el.name);
            if (!inst) continue;
            const parentName = inst.expansion_parent;
            if (!parentName) {
                // Check if this instance IS a parent (has expansion children)
                const hasChildren = schematicData.instances.some(
                    i => i.expansion_parent === el.name
                );
                if (!hasChildren) continue;
                if (!groups.has(el.name)) groups.set(el.name, []);
                groups.get(el.name).push(el);
            } else {
                if (!groups.has(parentName)) groups.set(parentName, []);
                groups.get(parentName).push(el);
            }
        }

        for (const [parentName, elements] of groups) {
            if (elements.length < 2) continue; // need parent + at least one child
            let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
            for (const el of elements) {
                minX = Math.min(minX, el.x - PAD);
                minY = Math.min(minY, el.y - PAD - LABEL_H);
                maxX = Math.max(maxX, el.x + el.w + PAD);
                maxY = Math.max(maxY, el.y + el.h + PAD);
            }
            // Draw dashed rounded rectangle
            const r = 8;
            ctx.save();
            ctx.setLineDash([6, 4]);
            ctx.strokeStyle = '#555';
            ctx.lineWidth = 1;
            ctx.globalAlpha = 0.6;
            ctx.beginPath();
            ctx.moveTo(minX + r, minY);
            ctx.lineTo(maxX - r, minY);
            ctx.arcTo(maxX, minY, maxX, minY + r, r);
            ctx.lineTo(maxX, maxY - r);
            ctx.arcTo(maxX, maxY, maxX - r, maxY, r);
            ctx.lineTo(minX + r, maxY);
            ctx.arcTo(minX, maxY, minX, maxY - r, r);
            ctx.lineTo(minX, minY + r);
            ctx.arcTo(minX, minY, minX + r, minY, r);
            ctx.closePath();
            ctx.stroke();
            // Label
            ctx.setLineDash([]);
            ctx.globalAlpha = 0.5;
            ctx.font = `${FONT_SIZE - 1}px monospace`;
            ctx.fillStyle = COLORS.textDim;
            ctx.textAlign = 'left';
            ctx.textBaseline = 'bottom';
            ctx.fillText(parentName + ' expansion', minX + 6, minY + LABEL_H - 2);
            ctx.restore();
        }
    }

    function render() {
        if (!ctx || !schematicData) return;
        const dpr = window.devicePixelRatio || 1;
        const w = canvas.clientWidth, h = canvas.clientHeight;
        if (w <= 0 || h <= 0) return;
        canvas.width = w * dpr;
        canvas.height = h * dpr;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.fillStyle = '#1e1e1e';
        ctx.fillRect(0, 0, w, h);
        ctx.save();
        ctx.translate(panX, panY);
        ctx.scale(zoomLevel, zoomLevel);

        drawExpansionGroups();
        drawWires();
        for (const el of layoutElements) {
            if (el.type === 'entity_in' || el.type === 'entity_out') drawEntityBox(el);
            else if (el.type === 'power_source') drawPowerSourceNode(el);
            else if (isSymbolCategory(el.category)) drawSymbolComponent(el);
            else drawInstanceBox(el);
        }
        // Draw GND and power stubs on top
        for (const el of layoutElements) {
            if (el.gndStubs && el.gndStubs.length > 0) drawGndStubs(el);
            if (el.pwrStubs && el.pwrStubs.length > 0) drawPowerStubs(el);
        }

        ctx.restore();

        // Draw simulation tooltip for hovered items (in screen space)
        drawSimTooltip();
    }

    function drawSimTooltip() {
        const sim = schematicData?.simulation;
        const lines = [];

        if (hoveredItem) {
            const el = layoutElements.find(e => e.name === hoveredItem);
            if (el && el.type === 'instance') {
                // Show both user handle and refdes in hover
                const nameLabel = el.refdes ? el.name + '  [' + el.refdes + ']' : el.name;
                lines.push(nameLabel + ' (' + (el.entityType || '') + ')');
                if (sim) {
                    if (el.simCurrent != null) lines.push('I = ' + formatCurrent(el.simCurrent));
                    if (el.simPower != null) lines.push('P = ' + formatPower(el.simPower));
                }
                // Show non-inline params on hover
                if (el.parameters) {
                    for (const [k, v] of el.parameters) {
                        if (!INLINE_PARAM_KEYS.has(k) && v) lines.push(k + ': ' + v);
                    }
                }
            }
        } else if (hoveredNet && sim) {
            lines.push(hoveredNet);
            const v = sim.net_voltages?.[hoveredNet];
            if (v != null) lines.push('V = ' + formatVoltage(v));
            const isPwr = sim.power_nets && (
                sim.power_nets instanceof Set ? sim.power_nets.has(hoveredNet) : !!sim.power_nets[hoveredNet]
            );
            lines.push(isPwr ? 'Class: power' : 'Class: signal');
        }

        if (lines.length === 0) return;

        const px = tooltipScreenX, py = tooltipScreenY;
        const padX = 8, padY = 5;
        ctx.font = `${FONT_SIZE - 1}px monospace`;
        let maxW = 0;
        for (const l of lines) maxW = Math.max(maxW, ctx.measureText(l).width);
        const boxW = maxW + padX * 2;
        const boxH = lines.length * (FONT_SIZE + 2) + padY * 2;
        const bx = px + 12, by = py - boxH - 4;

        ctx.fillStyle = 'rgba(30,30,30,0.92)';
        ctx.strokeStyle = '#666';
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.roundRect(bx, by, boxW, boxH, 4);
        ctx.fill();
        ctx.stroke();

        ctx.fillStyle = '#ddd';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        for (let i = 0; i < lines.length; i++) {
            ctx.fillStyle = i === 0 ? '#fff' : '#bbb';
            ctx.fillText(lines[i], bx + padX, by + padY + i * (FONT_SIZE + 2));
        }
    }

    let tooltipScreenX = 0, tooltipScreenY = 0;

    function drawPowerSourceNode(el) {
        drawRoundedRect(ctx, el.x, el.y, el.w, el.h, 3, COLORS.powerSrcBg, COLORS.powerSrcBorder, 1.5);
        ctx.fillStyle = COLORS.powerSrcText;
        ctx.font = `bold ${FONT_SIZE}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(el.label, el.x + el.w / 2, el.y + el.h / 2);

        for (const port of el.outputPorts) {
            ctx.strokeStyle = COLORS.powerSrcBorder;
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.moveTo(port.x, port.y);
            ctx.lineTo(port.x + PORT_STUB_LEN, port.y);
            ctx.stroke();
            ctx.fillStyle = COLORS.powerSrcBorder;
            ctx.beginPath();
            ctx.arc(port.x + PORT_STUB_LEN, port.y, PORT_DOT_R, 0, Math.PI * 2);
            ctx.fill();
        }
    }

    function drawEntityBox(el) {
        const isInput = el.type === 'entity_in';
        drawRoundedRect(ctx, el.x, el.y, el.w, el.h, BORDER_RADIUS, COLORS.entityBg, COLORS.entityBorder, 2);

        // Header
        ctx.save();
        ctx.beginPath();
        ctx.moveTo(el.x + BORDER_RADIUS, el.y);
        ctx.lineTo(el.x + el.w - BORDER_RADIUS, el.y);
        ctx.quadraticCurveTo(el.x + el.w, el.y, el.x + el.w, el.y + BORDER_RADIUS);
        ctx.lineTo(el.x + el.w, el.y + HEADER_HEIGHT);
        ctx.lineTo(el.x, el.y + HEADER_HEIGHT);
        ctx.lineTo(el.x, el.y + BORDER_RADIUS);
        ctx.quadraticCurveTo(el.x, el.y, el.x + BORDER_RADIUS, el.y);
        ctx.closePath();
        ctx.fillStyle = COLORS.entityHeader;
        ctx.fill();
        ctx.restore();

        ctx.fillStyle = COLORS.entityBorder;
        ctx.font = `bold ${FONT_SIZE}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(isInput ? el.name + ' (in)' : el.name + ' (out)', el.x + el.w / 2, el.y + HEADER_HEIGHT / 2);

        const ports = isInput ? el.outputPorts : el.inputPorts;
        for (const port of ports) {
            const color = port.isClock ? COLORS.portClock : port.isReset ? COLORS.portReset : COLORS.port;
            ctx.strokeStyle = color;
            ctx.lineWidth = 2;
            ctx.beginPath();
            if (isInput) { ctx.moveTo(port.x, port.y); ctx.lineTo(port.x + PORT_STUB_LEN, port.y); }
            else { ctx.moveTo(port.x - PORT_STUB_LEN, port.y); ctx.lineTo(port.x, port.y); }
            ctx.stroke();
            ctx.fillStyle = color;
            ctx.beginPath();
            ctx.arc(isInput ? port.x + PORT_STUB_LEN : port.x - PORT_STUB_LEN, port.y, PORT_DOT_R, 0, Math.PI * 2);
            ctx.fill();
            ctx.fillStyle = COLORS.text;
            ctx.font = `${FONT_SIZE}px monospace`;
            ctx.textAlign = isInput ? 'right' : 'left';
            ctx.fillText(port.name, isInput ? port.x - 4 : port.x + 4, port.y + 4);
        }
    }

    function drawInstanceBox(el) {
        const isHovered = hoveredItem === el.name;
        const borderColor = isHovered ? COLORS.highlight : COLORS.instanceBorder;
        const lw = isHovered ? 2 : 1;

        drawRoundedRect(ctx, el.x, el.y, el.w, el.h, BORDER_RADIUS, COLORS.instanceBg, borderColor, lw);

        // Header
        ctx.save();
        ctx.beginPath();
        ctx.moveTo(el.x + BORDER_RADIUS, el.y);
        ctx.lineTo(el.x + el.w - BORDER_RADIUS, el.y);
        ctx.quadraticCurveTo(el.x + el.w, el.y, el.x + el.w, el.y + BORDER_RADIUS);
        ctx.lineTo(el.x + el.w, el.y + HEADER_HEIGHT);
        ctx.lineTo(el.x, el.y + HEADER_HEIGHT);
        ctx.lineTo(el.x, el.y + BORDER_RADIUS);
        ctx.quadraticCurveTo(el.x, el.y, el.x + BORDER_RADIUS, el.y);
        ctx.closePath();
        ctx.fillStyle = COLORS.instanceHeader;
        ctx.fill();
        ctx.restore();

        // Instance name (uses displayName which toggles between user handle and refdes)
        ctx.fillStyle = COLORS.text;
        ctx.font = `bold ${FONT_SIZE}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(el.displayName || el.name, el.x + el.w / 2, el.y + HEADER_HEIGHT / 2);

        // Entity type
        if (el.entityType) {
            ctx.fillStyle = COLORS.textDim;
            ctx.font = `${FONT_SIZE - 1}px monospace`;
            ctx.fillText(el.entityType, el.x + el.w / 2, el.y + HEADER_HEIGHT + 10);
        }

        // Parameters — only show value inline; rest on hover
        if (el.parameters && el.parameters.length > 0) {
            ctx.fillStyle = COLORS.paramText;
            ctx.font = `${FONT_SIZE - 2}px monospace`;
            const paramStr = el.parameters.filter(p => p[1] && INLINE_PARAM_KEYS.has(p[0])).map(p => formatParamValue(p[1])).join(', ');
            if (paramStr) ctx.fillText(paramStr, el.x + el.w / 2, el.y + HEADER_HEIGHT + 22);
        }

        const showLabels = shouldShowPortLabels(el.category);

        if (el.isShunt) {
            // NORTH port: wire drops down from above
            for (const port of el.inputPorts) {
                ctx.strokeStyle = COLORS.port;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(port.x, port.y - PORT_STUB_LEN);
                ctx.stroke();
                ctx.fillStyle = COLORS.port;
                ctx.beginPath();
                ctx.arc(port.x, port.y - PORT_STUB_LEN, PORT_DOT_R, 0, Math.PI * 2);
                ctx.fill();
            }
            // SOUTH port: chain shunt output continues down to next component
            for (const port of el.outputPorts) {
                ctx.strokeStyle = COLORS.port;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(port.x, port.y + PORT_STUB_LEN);
                ctx.stroke();
                ctx.fillStyle = COLORS.port;
                ctx.beginPath();
                ctx.arc(port.x, port.y + PORT_STUB_LEN, PORT_DOT_R, 0, Math.PI * 2);
                ctx.fill();
            }
            return; // Skip standard left/right port rendering
        }

        function portColor(port) {
            if (port.pinType === 'power') return COLORS.portPower;
            if (port.isClock || port.pinType === 'clock') return COLORS.portClock;
            if (port.isReset || port.pinType === 'reset') return COLORS.portReset;
            return COLORS.port;
        }

        // Input ports — normally left, but right if flipped
        const inDir = el.isFlipped ? 1 : -1;  // stub direction: -1 = left, +1 = right
        for (const port of el.inputPorts) {
            const pc = portColor(port);
            ctx.strokeStyle = pc;
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.moveTo(port.x, port.y);
            ctx.lineTo(port.x + inDir * PORT_STUB_LEN, port.y);
            ctx.stroke();
            ctx.fillStyle = pc;
            ctx.beginPath();
            ctx.arc(port.x + inDir * PORT_STUB_LEN, port.y, PORT_DOT_R - 0.5, 0, Math.PI * 2);
            ctx.fill();
            if (showLabels) {
                ctx.fillStyle = COLORS.text;
                ctx.font = `${FONT_SIZE - 1}px monospace`;
                ctx.textAlign = el.isFlipped ? 'right' : 'left';
                ctx.textBaseline = 'middle';
                const labelX = el.isFlipped ? port.x - 4 : port.x + 4;
                ctx.fillText(port.name, labelX, port.y);
            }
        }

        // Output ports — normally right, but left if flipped
        const outDir = el.isFlipped ? -1 : 1;
        for (const port of el.outputPorts) {
            const pc = portColor(port);
            ctx.strokeStyle = pc;
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.moveTo(port.x, port.y);
            ctx.lineTo(port.x + outDir * PORT_STUB_LEN, port.y);
            ctx.stroke();
            ctx.fillStyle = pc;
            ctx.beginPath();
            ctx.arc(port.x + outDir * PORT_STUB_LEN, port.y, PORT_DOT_R - 0.5, 0, Math.PI * 2);
            ctx.fill();
            if (showLabels) {
                ctx.fillStyle = COLORS.text;
                ctx.font = `${FONT_SIZE - 1}px monospace`;
                ctx.textAlign = el.isFlipped ? 'left' : 'right';
                ctx.textBaseline = 'middle';
                const labelX = el.isFlipped ? port.x + 4 : port.x - 4;
                ctx.fillText(port.name, labelX, port.y);
            }
        }
    }

    // ─────────── PROFESSIONAL SYMBOL RENDERING ───────────

    function drawResistorSymbol(cx, cy, vertical) {
        // European/IEC style: simple rectangle
        const rw = 20, rh = 7;
        if (vertical) {
            ctx.strokeRect(cx - rh, cy - rw, rh * 2, rw * 2);
        } else {
            ctx.strokeRect(cx - rw, cy - rh, rw * 2, rh * 2);
        }
    }

    function drawCapacitorSymbol(cx, cy, vertical) {
        // Two parallel lines with a gap
        const plateLen = 10, gap = 3;
        if (vertical) {
            // Plates are horizontal, stacked vertically
            ctx.beginPath();
            ctx.moveTo(cx - plateLen, cy - gap);
            ctx.lineTo(cx + plateLen, cy - gap);
            ctx.stroke();
            ctx.beginPath();
            ctx.moveTo(cx - plateLen, cy + gap);
            ctx.lineTo(cx + plateLen, cy + gap);
            ctx.stroke();
        } else {
            // Plates are vertical, side by side
            ctx.beginPath();
            ctx.moveTo(cx - gap, cy - plateLen);
            ctx.lineTo(cx - gap, cy + plateLen);
            ctx.stroke();
            ctx.beginPath();
            ctx.moveTo(cx + gap, cy - plateLen);
            ctx.lineTo(cx + gap, cy + plateLen);
            ctx.stroke();
        }
    }

    function drawInductorSymbol(cx, cy, vertical) {
        // Series of 4 semicircular humps
        const humps = 4, humpR = 5;
        const halfLen = humps * humpR;
        if (vertical) {
            const startY = cy - halfLen;
            for (let i = 0; i < humps; i++) {
                const hcy = startY + (i + 0.5) * (humpR * 2);
                ctx.beginPath();
                ctx.arc(cx, hcy, humpR, -Math.PI / 2, Math.PI / 2, false);
                ctx.stroke();
            }
        } else {
            const startX = cx - halfLen;
            for (let i = 0; i < humps; i++) {
                const hcx = startX + (i + 0.5) * (humpR * 2);
                ctx.beginPath();
                ctx.arc(hcx, cy, humpR, Math.PI, 0, false);
                ctx.stroke();
            }
        }
    }

    function drawDiodeSymbol(cx, cy, vertical, isLED, isFlipped) {
        // Triangle + bar, centered on (cx, cy)
        const triW = 10, triH = 7;
        const half = triW / 2; // center offset so symbol is symmetric about cx/cy
        const dir = isFlipped ? -1 : 1;
        if (vertical) {
            const vdir = isFlipped ? -1 : 1;
            // Triangle pointing down, centered vertically
            const anode = cy - half * vdir;   // triangle base (anode side)
            const cathode = cy + half * vdir; // triangle tip + bar (cathode side)
            ctx.beginPath();
            ctx.moveTo(cx - triH, anode);
            ctx.lineTo(cx + triH, anode);
            ctx.lineTo(cx, cathode);
            ctx.closePath();
            ctx.fill();
            // Bar at cathode
            ctx.beginPath();
            ctx.moveTo(cx - triH, cathode);
            ctx.lineTo(cx + triH, cathode);
            ctx.stroke();
        } else {
            // Triangle pointing right (or left if flipped), centered horizontally
            const anode = cx - half * dir;   // triangle base
            const cathode = cx + half * dir; // triangle tip + bar
            ctx.beginPath();
            ctx.moveTo(anode, cy - triH);
            ctx.lineTo(anode, cy + triH);
            ctx.lineTo(cathode, cy);
            ctx.closePath();
            ctx.fill();
            // Bar at cathode
            ctx.beginPath();
            ctx.moveTo(cathode, cy - triH);
            ctx.lineTo(cathode, cy + triH);
            ctx.stroke();
        }
        // LED photon emission arrows (two small arrows radiating away from junction)
        if (isLED) {
            ctx.lineWidth = 1;
            const aLen = 8, headLen = 3;
            if (vertical) {
                // Arrows radiate to the right from the junction
                for (let i = 0; i < 2; i++) {
                    const ay = cy - 3 + i * 6;
                    const ax = cx + triH + 2;
                    // Shaft
                    ctx.beginPath();
                    ctx.moveTo(ax, ay);
                    ctx.lineTo(ax + aLen, ay - aLen * 0.6);
                    ctx.stroke();
                    // Arrowhead
                    const tipX = ax + aLen, tipY = ay - aLen * 0.6;
                    ctx.beginPath();
                    ctx.moveTo(tipX, tipY);
                    ctx.lineTo(tipX - headLen, tipY + 1);
                    ctx.moveTo(tipX, tipY);
                    ctx.lineTo(tipX - 1, tipY + headLen);
                    ctx.stroke();
                }
            } else {
                // Arrows radiate upward from the junction
                for (let i = 0; i < 2; i++) {
                    const ax = cx - 3 * dir + i * 6 * dir;
                    const ay = cy - triH - 2;
                    // Shaft
                    ctx.beginPath();
                    ctx.moveTo(ax, ay);
                    ctx.lineTo(ax + aLen * 0.4 * dir, ay - aLen);
                    ctx.stroke();
                    // Arrowhead
                    const tipX = ax + aLen * 0.4 * dir, tipY = ay - aLen;
                    ctx.beginPath();
                    ctx.moveTo(tipX, tipY);
                    ctx.lineTo(tipX - headLen * 0.5 * dir, tipY + headLen);
                    ctx.moveTo(tipX, tipY);
                    ctx.lineTo(tipX - headLen * dir, tipY + headLen * 0.5);
                    ctx.stroke();
                }
            }
        }
    }

    function drawTVSDiodeSymbol(cx, cy, vertical) {
        // Bidirectional: two triangles pointing at each other
        const triW = 6, triH = 7;
        if (vertical) {
            // Top triangle pointing down
            ctx.beginPath();
            ctx.moveTo(cx - triH, cy - triW);
            ctx.lineTo(cx + triH, cy - triW);
            ctx.lineTo(cx, cy);
            ctx.closePath();
            ctx.fill();
            // Bottom triangle pointing up
            ctx.beginPath();
            ctx.moveTo(cx - triH, cy + triW);
            ctx.lineTo(cx + triH, cy + triW);
            ctx.lineTo(cx, cy);
            ctx.closePath();
            ctx.fill();
            // Bars
            ctx.beginPath();
            ctx.moveTo(cx - triH, cy);
            ctx.lineTo(cx + triH, cy);
            ctx.stroke();
        } else {
            // Left triangle pointing right
            ctx.beginPath();
            ctx.moveTo(cx - triW, cy - triH);
            ctx.lineTo(cx - triW, cy + triH);
            ctx.lineTo(cx, cy);
            ctx.closePath();
            ctx.fill();
            // Right triangle pointing left
            ctx.beginPath();
            ctx.moveTo(cx + triW, cy - triH);
            ctx.lineTo(cx + triW, cy + triH);
            ctx.lineTo(cx, cy);
            ctx.closePath();
            ctx.fill();
            // Bar in center
            ctx.beginPath();
            ctx.moveTo(cx, cy - triH);
            ctx.lineTo(cx, cy + triH);
            ctx.stroke();
        }
    }

    function drawOpAmpSymbol(cx, cy, isFlipped, boundH) {
        // Triangle with +/- labels
        const w = 25, h = 22;
        const dir = isFlipped ? -1 : 1;
        ctx.beginPath();
        // Triangle: flat on left (input side), point on right (output)
        ctx.moveTo(cx - w * dir, cy - h);
        ctx.lineTo(cx - w * dir, cy + h);
        ctx.lineTo(cx + w * dir, cy);
        ctx.closePath();
        ctx.stroke();

        // Power/GND stub leads from triangle body to bounding box edge
        // The triangle slopes linearly: at x=cx, the top edge is at cy - h/2
        // and the bottom edge is at cy + h/2 (midpoint of sloped sides).
        const triYAtCenter = h / 2;
        const halfBound = boundH / 2;
        // Bottom lead (GND): from triangle bottom edge at cx to bounding box bottom
        ctx.beginPath();
        ctx.moveTo(cx, cy + triYAtCenter);
        ctx.lineTo(cx, cy + halfBound);
        ctx.stroke();
        // Top lead (VCC): from triangle top edge at cx to bounding box top
        ctx.beginPath();
        ctx.moveTo(cx, cy - triYAtCenter);
        ctx.lineTo(cx, cy - halfBound);
        ctx.stroke();

        // +/- labels inside the triangle
        ctx.fillStyle = COLORS.text;
        ctx.font = `bold ${FONT_SIZE}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        const labelX = cx - w * dir * 0.5;
        ctx.fillText('+', labelX, cy - 10);
        ctx.fillText('\u2212', labelX, cy + 10);
    }

    function drawSymbolComponent(el) {
        const isHovered = hoveredItem === el.name;
        const cx = el.x + el.w / 2;
        const cy = el.y + el.h / 2;
        const cat = el.category;
        const isFlipped = el.isFlipped;
        const isVertical = el.isShunt;

        // Symbol stroke color
        const symbolColor = isHovered ? COLORS.highlight : '#c0c0c0';
        ctx.strokeStyle = symbolColor;
        ctx.fillStyle = symbolColor;
        ctx.lineWidth = 1.5;
        ctx.lineCap = 'round';
        ctx.lineJoin = 'round';

        // Lead lines from bounding box edge to symbol body
        // When vertical, the symbol rotates so bodyW becomes the vertical extent
        const ss = SYMBOL_SIZES[cat];
        if (cat === 'opamp') {
            // OpAmp: leads from box edges to triangle body
            const w = 25, dir = isFlipped ? -1 : 1;
            const inputX = cx - w * dir;  // flat side of triangle
            const outputX = cx + w * dir; // tip of triangle
            // Input leads (INP at cy-10, INM at cy+10)
            for (const port of el.inputPorts) {
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(inputX, port.y);
                ctx.stroke();
            }
            // Output lead
            for (const port of el.outputPorts) {
                ctx.beginPath();
                ctx.moveTo(outputX, port.y);
                ctx.lineTo(port.x, port.y);
                ctx.stroke();
            }
        } else {
            const bodyLong = ss.bodyW, bodyShort = ss.bodyH;
            if (isVertical) {
                // Vertical: bodyW runs along Y axis
                const bodyTop = cy - bodyLong / 2;
                const bodyBot = cy + bodyLong / 2;
                ctx.beginPath();
                ctx.moveTo(cx, el.y);
                ctx.lineTo(cx, bodyTop);
                ctx.stroke();
                ctx.beginPath();
                ctx.moveTo(cx, bodyBot);
                ctx.lineTo(cx, el.y + el.h);
                ctx.stroke();
            } else {
                // Horizontal: bodyW runs along X axis
                const bodyLeft = cx - bodyLong / 2;
                const bodyRight = cx + bodyLong / 2;
                ctx.beginPath();
                ctx.moveTo(el.x, cy);
                ctx.lineTo(bodyLeft, cy);
                ctx.stroke();
                ctx.beginPath();
                ctx.moveTo(bodyRight, cy);
                ctx.lineTo(el.x + el.w, cy);
                ctx.stroke();
            }
        }

        // Draw the actual symbol
        if (cat === 'resistor') {
            drawResistorSymbol(cx, cy, isVertical);
        } else if (cat === 'capacitor') {
            drawCapacitorSymbol(cx, cy, isVertical);
        } else if (cat === 'inductor') {
            drawInductorSymbol(cx, cy, isVertical);
        } else if (cat === 'diode') {
            const entityLower = (el.entityType || '').toLowerCase();
            const isLED = entityLower.startsWith('led');
            drawDiodeSymbol(cx, cy, isVertical, isLED, isFlipped);
        } else if (cat === 'protection') {
            drawTVSDiodeSymbol(cx, cy, isVertical);
        } else if (cat === 'opamp') {
            drawOpAmpSymbol(cx, cy, isFlipped, el.h);
        }

        // Port stubs and dots
        if (el.isShunt) {
            // Shunt: NORTH port (input from above)
            for (const port of el.inputPorts) {
                ctx.strokeStyle = COLORS.port;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(port.x, port.y - PORT_STUB_LEN);
                ctx.stroke();
                ctx.fillStyle = COLORS.port;
                ctx.beginPath();
                ctx.arc(port.x, port.y - PORT_STUB_LEN, PORT_DOT_R, 0, Math.PI * 2);
                ctx.fill();
            }
            // SOUTH port (chain shunt output)
            for (const port of el.outputPorts) {
                ctx.strokeStyle = COLORS.port;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(port.x, port.y + PORT_STUB_LEN);
                ctx.stroke();
                ctx.fillStyle = COLORS.port;
                ctx.beginPath();
                ctx.arc(port.x, port.y + PORT_STUB_LEN, PORT_DOT_R, 0, Math.PI * 2);
                ctx.fill();
            }
        } else {
            // Horizontal: L/R port stubs
            const inDir = isFlipped ? 1 : -1;
            for (const port of el.inputPorts) {
                ctx.strokeStyle = COLORS.port;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(port.x + inDir * PORT_STUB_LEN, port.y);
                ctx.stroke();
                ctx.fillStyle = COLORS.port;
                ctx.beginPath();
                ctx.arc(port.x + inDir * PORT_STUB_LEN, port.y, PORT_DOT_R, 0, Math.PI * 2);
                ctx.fill();
            }
            const outDir = isFlipped ? -1 : 1;
            for (const port of el.outputPorts) {
                ctx.strokeStyle = COLORS.port;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(port.x + outDir * PORT_STUB_LEN, port.y);
                ctx.stroke();
                ctx.fillStyle = COLORS.port;
                ctx.beginPath();
                ctx.arc(port.x + outDir * PORT_STUB_LEN, port.y, PORT_DOT_R, 0, Math.PI * 2);
                ctx.fill();
            }
        }

        // Labels: name above, value below (or inside for resistors)
        const paramStr = (el.parameters && el.parameters.length > 0)
            ? el.parameters.filter(p => p[1] && INLINE_PARAM_KEYS.has(p[0])).map(p => formatParamValue(p[1])).join(', ') : '';
        const valueInside = cat === 'resistor' && paramStr;

        ctx.fillStyle = COLORS.text;
        ctx.font = `bold ${FONT_SIZE - 1}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        const labelAboveY = isVertical ? cy : el.y - 2;
        const labelAboveX = isVertical ? el.x - 4 : cx;
        if (isVertical) {
            ctx.textAlign = 'right';
            ctx.textBaseline = 'middle';
        }
        ctx.fillText(el.displayName || el.name, labelAboveX, labelAboveY);

        // Value: inside the rectangle for resistors, below for others
        if (valueInside) {
            ctx.fillStyle = COLORS.text;
            ctx.font = `${FONT_SIZE - 2}px monospace`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            if (isVertical) {
                // Rotate text 90° and draw inside the vertical resistor body
                ctx.save();
                ctx.translate(cx, cy);
                ctx.rotate(-Math.PI / 2);
                ctx.fillText(paramStr, 0, 0);
                ctx.restore();
            } else {
                ctx.fillText(paramStr, cx, cy);
            }
        } else if (paramStr) {
            ctx.fillStyle = COLORS.paramText;
            ctx.font = `${FONT_SIZE - 2}px monospace`;
            if (isVertical) {
                ctx.textAlign = 'right';
                ctx.textBaseline = 'middle';
                ctx.fillText(paramStr, el.x - 4, cy + FONT_SIZE + 2);
            } else {
                ctx.textAlign = 'center';
                ctx.textBaseline = 'top';
                ctx.fillText(paramStr, cx, el.y + el.h + 2);
            }
        }

    }

    function drawGndStubs(el) {
        if (!el.gndStubs || el.gndStubs.length === 0) return;
        const count = el.gndStubs.length;
        const spacing = el.w / (count + 1);

        for (let i = 0; i < count; i++) {
            const cx = el.x + spacing * (i + 1);
            const botY = el.y + el.h;
            // If gndTargetY is set, extend the stub wire to reach the common bottom
            const targetY = el.gndTargetY || botY;
            const stubStartY = Math.max(botY, targetY);

            // Vertical line down (may be extended to align with other columns)
            ctx.strokeStyle = COLORS.groundStub;
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.moveTo(cx, botY);
            ctx.lineTo(cx, stubStartY + GND_STUB_HEIGHT);
            ctx.stroke();

            // Ground symbol: 3 narrowing horizontal lines
            for (let g = 0; g < GND_LINE_WIDTHS.length; g++) {
                const lw = GND_LINE_WIDTHS[g];
                const ly = stubStartY + GND_STUB_HEIGHT + g * GND_LINE_SPACING;
                ctx.beginPath();
                ctx.moveTo(cx - lw / 2, ly);
                ctx.lineTo(cx + lw / 2, ly);
                ctx.stroke();
            }

            // Connection dot at box edge
            ctx.fillStyle = COLORS.groundStub;
            ctx.beginPath();
            ctx.arc(cx, botY, 2, 0, Math.PI * 2);
            ctx.fill();
        }
    }

    function drawPowerStubs(el) {
        if (!el.pwrStubs || el.pwrStubs.length === 0) return;
        const count = el.pwrStubs.length;
        const spacing = el.w / (count + 1);
        const PWR_STUB_HEIGHT = 18;
        const PWR_BAR_WIDTH = 12;

        for (let i = 0; i < count; i++) {
            const cx = el.x + spacing * (i + 1);
            const topY = el.y;

            // Vertical line up
            ctx.strokeStyle = COLORS.portPower;
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.moveTo(cx, topY);
            ctx.lineTo(cx, topY - PWR_STUB_HEIGHT);
            ctx.stroke();

            // Power bar at top
            ctx.beginPath();
            ctx.moveTo(cx - PWR_BAR_WIDTH / 2, topY - PWR_STUB_HEIGHT);
            ctx.lineTo(cx + PWR_BAR_WIDTH / 2, topY - PWR_STUB_HEIGHT);
            ctx.stroke();

            // Voltage label
            const stub = el.pwrStubs[i];
            const label = stub.voltage != null ? formatVoltage(stub.voltage) : (stub.netName || 'VCC');
            ctx.fillStyle = COLORS.portPower;
            ctx.font = `${FONT_SIZE - 2}px monospace`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'bottom';
            ctx.fillText(label, cx, topY - PWR_STUB_HEIGHT - 2);

            // Connection dot at box edge
            ctx.fillStyle = COLORS.portPower;
            ctx.beginPath();
            ctx.arc(cx, topY, 2, 0, Math.PI * 2);
            ctx.fill();
        }
    }

    function drawWires() {
        const voltageAnnotatedNets = new Set(); // deduplicate voltage labels per net
        const wireElByName = new Map();
        for (const el of layoutElements) wireElByName.set(el.name, el);
        for (const wire of layoutWires) {
            const isBus = wire.width > 1;
            const isHighlighted = hoveredNet === wire.netName;
            const isPower = wire.isPower;
            const isClock = clockSignals.has(wire.netName);
            const isReset = resetSignals.has(wire.netName);

            const color = isHighlighted ? COLORS.wireHighlight :
                          isPower ? COLORS.portPower :
                          isClock ? COLORS.portClock :
                          isReset ? COLORS.portReset :
                          isBus ? COLORS.wireBus : COLORS.wire;
            const lineW = isBus ? 2.5 : isPower ? 1.5 : 1;

            ctx.strokeStyle = color;
            ctx.lineWidth = lineW;
            ctx.lineCap = 'round';
            ctx.lineJoin = 'round';

            for (const seg of wire.segments) {
                ctx.beginPath();
                ctx.moveTo(seg.x1, seg.y1);
                ctx.lineTo(seg.x2, seg.y2);
                ctx.stroke();
            }

            // Bus annotation
            if (isBus && wire.segments.length > 0) {
                const seg = wire.segments[0];
                const mx = (seg.x1 + seg.x2) / 2, my = (seg.y1 + seg.y2) / 2;
                ctx.strokeStyle = COLORS.busSlash;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(mx - 4, my + 5);
                ctx.lineTo(mx + 4, my - 5);
                ctx.stroke();
                ctx.fillStyle = COLORS.busLabel;
                ctx.font = `${FONT_SIZE - 2}px monospace`;
                ctx.textAlign = 'center';
                ctx.textBaseline = 'bottom';
                ctx.fillText(String(wire.width), mx + 8, my - 3);
            }

            // Junction dots — match wire color
            ctx.fillStyle = color;
            ctx.beginPath();
            ctx.arc(wire.from.x, wire.from.y, isBus ? 3 : 2, 0, Math.PI * 2);
            ctx.fill();
            ctx.beginPath();
            ctx.arc(wire.to.x, wire.to.y, isBus ? 3 : 2, 0, Math.PI * 2);
            ctx.fill();

            // ── Voltage & Current annotations ──
            // Find the longest segment for annotation placement.
            // For shunt wires, prefer vertical segment so labels don't overlap
            // with the main-path horizontal wire they branch from.
            const sinkEl = wireElByName.get(wire.sinkElName || '');
            const isShuntLikeWire = sinkEl && sinkEl.isShunt;
            let bestSeg = null, bestLen = 0;
            if (wire.segments.length > 0) {
                for (const s of wire.segments) {
                    const hLen = Math.abs(s.x2 - s.x1);
                    const vLen = Math.abs(s.y2 - s.y1);
                    const isVert = vLen > hLen;
                    const len = Math.max(hLen, vLen);
                    // For shunt wires, prefer vertical segments to avoid
                    // overlapping annotations with the main-path wire
                    const score = (isShuntLikeWire && isVert) ? len + 10000 : len;
                    if (score > bestLen) { bestLen = score; bestSeg = s; }
                }
                // Restore bestLen to actual length for threshold checks
                bestLen = bestSeg ? Math.max(Math.abs(bestSeg.x2 - bestSeg.x1), Math.abs(bestSeg.y2 - bestSeg.y1)) : 0;
            }

            // ── Voltage annotation ──
            // Show voltage once per net. For shunt wires, show on the first wire's
            // longest horizontal segment (the shared "trunk" from the driver).
            const alreadyAnnotated = voltageAnnotatedNets.has(wire.netName);
            const showVoltage = wire.voltage != null && (!alreadyAnnotated || !isShuntLikeWire);

            // Find the longest horizontal segment for trunk annotations
            const horizSeg = wire.segments.reduce((best, s) => {
                const len = Math.abs(s.x2 - s.x1);
                return (len > Math.abs(s.y2 - s.y1) && len > (best ? Math.abs(best.x2 - best.x1) : 0)) ? s : best;
            }, null);
            // Find the longest vertical segment for branch annotations
            const vertSeg = wire.segments.reduce((best, s) => {
                const len = Math.abs(s.y2 - s.y1);
                return (len > Math.abs(s.x2 - s.x1) && len > (best ? Math.abs(best.y2 - best.y1) : 0)) ? s : best;
            }, null);

            if (showVoltage && bestSeg && bestLen > 20) {
                voltageAnnotatedNets.add(wire.netName);
                // For shunt wires, prefer the horizontal trunk segment for voltage
                const vSeg = (isShuntLikeWire && horizSeg && Math.abs(horizSeg.x2 - horizSeg.x1) > 30) ? horizSeg : bestSeg;
                const segIsHoriz = Math.abs(vSeg.x2 - vSeg.x1) >= Math.abs(vSeg.y2 - vSeg.y1);
                ctx.font = `${FONT_SIZE - 2}px monospace`;
                ctx.fillStyle = isPower ? COLORS.portPower
                    : isHighlighted ? COLORS.wireHighlight
                    : '#90caf9';
                if (segIsHoriz) {
                    const lx = (vSeg.x1 + vSeg.x2) / 2;
                    ctx.textAlign = 'center';
                    ctx.textBaseline = 'bottom';
                    ctx.fillText(formatVoltage(wire.voltage), lx, vSeg.y1 - 5);
                } else {
                    const cy = (vSeg.y1 + vSeg.y2) / 2;
                    ctx.textAlign = 'right';
                    ctx.textBaseline = 'middle';
                    ctx.fillText(formatVoltage(wire.voltage), vSeg.x1 - 6, cy);
                }
            } else if (!wire.driverIsPowerSource && !voltageAnnotatedNets.has(wire.netName) && wire.segments.length > 0) {
                // No simulation voltage — show net name on first long horizontal segment
                voltageAnnotatedNets.add(wire.netName);
                const seg = wire.segments.find(s => Math.abs(s.x2 - s.x1) > 60);
                if (seg) {
                    const lx = (seg.x1 + seg.x2) / 2;
                    ctx.font = `${FONT_SIZE - 2}px monospace`;
                    ctx.textAlign = 'center';
                    ctx.textBaseline = 'bottom';
                    ctx.fillStyle = COLORS.textMuted;
                    ctx.fillText(wire.netName, lx, seg.y1 - 5);
                }
            }

            // ── Current annotations ──
            // For shunt fan-out wires: show DRIVER current on the horizontal trunk
            // (once per net) and SINK current on the vertical drop (per wire).
            // For non-shunt wires: show sink current on the best segment.
            if (isShuntLikeWire) {
                // Trunk: driver's total current on horizontal segment (once per net)
                const trunkCurrent = wire.driverCurrent;
                if (!alreadyAnnotated && trunkCurrent != null && Math.abs(trunkCurrent) > 1e-3
                    && horizSeg && Math.abs(horizSeg.x2 - horizSeg.x1) > 30) {
                    const lx = (horizSeg.x1 + horizSeg.x2) / 2;
                    ctx.font = `${FONT_SIZE - 2}px monospace`;
                    ctx.fillStyle = '#8bc34a';
                    ctx.textAlign = 'center';
                    ctx.textBaseline = 'top';
                    ctx.fillText(formatCurrent(Math.abs(trunkCurrent)), lx, horizSeg.y1 + 4);
                }
                // Branch: sink's individual current on vertical segment
                if (wire.current != null && Math.abs(wire.current) > 1e-3
                    && vertSeg && Math.abs(vertSeg.y2 - vertSeg.y1) > 20) {
                    const cy = (vertSeg.y1 + vertSeg.y2) / 2;
                    ctx.font = `${FONT_SIZE - 2}px monospace`;
                    ctx.fillStyle = '#8bc34a';
                    ctx.textAlign = 'left';
                    ctx.textBaseline = 'middle';
                    ctx.fillText(formatCurrent(Math.abs(wire.current)), vertSeg.x1 + 6, cy);
                }
            } else if (wire.current != null && Math.abs(wire.current) > 1e-3 && bestSeg && bestLen > 20) {
                const segIsHoriz = Math.abs(bestSeg.x2 - bestSeg.x1) >= Math.abs(bestSeg.y2 - bestSeg.y1);
                ctx.font = `${FONT_SIZE - 2}px monospace`;
                ctx.fillStyle = '#8bc34a';
                if (segIsHoriz) {
                    const lx = (bestSeg.x1 + bestSeg.x2) / 2;
                    ctx.textAlign = 'center';
                    ctx.textBaseline = 'top';
                    ctx.fillText(formatCurrent(Math.abs(wire.current)), lx, bestSeg.y1 + 4);
                } else {
                    const cy = (bestSeg.y1 + bestSeg.y2) / 2;
                    ctx.textAlign = 'left';
                    ctx.textBaseline = 'middle';
                    ctx.fillText(formatCurrent(Math.abs(wire.current)), bestSeg.x1 + 6, cy);
                }
            }
        }

        // Draw T-junction dots (where shunt/branch wires meet main path)
        const junctionPts = layoutElements._junctionPoints || [];
        ctx.fillStyle = COLORS.junctionDot;
        for (const jp of junctionPts) {
            ctx.beginPath();
            ctx.arc(jp.x, jp.y, 4, 0, Math.PI * 2);
            ctx.fill();
        }

        // (Per-wire current annotations above are sufficient; no separate junction-total pass needed)
    }

    function drawRoundedRect(ctx, x, y, w, h, r, fill, stroke, lineWidth) {
        ctx.beginPath();
        ctx.moveTo(x + r, y);
        ctx.lineTo(x + w - r, y);
        ctx.quadraticCurveTo(x + w, y, x + w, y + r);
        ctx.lineTo(x + w, y + h - r);
        ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
        ctx.lineTo(x + r, y + h);
        ctx.quadraticCurveTo(x, y + h, x, y + h - r);
        ctx.lineTo(x, y + r);
        ctx.quadraticCurveTo(x, y, x + r, y);
        ctx.closePath();
        ctx.fillStyle = fill;
        ctx.fill();
        ctx.strokeStyle = stroke;
        ctx.lineWidth = lineWidth;
        ctx.stroke();
    }

    // ─────────── ZOOM TO FIT ───────────

    function zoomToFit() {
        if (layoutElements.length === 0) return;
        const padding = 50;
        let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;

        for (const el of layoutElements) {
            const hasNorthPort = el.isShunt;
            const hasGnd = el.gndStubs && el.gndStubs.length > 0;
            minX = Math.min(minX, el.x - (hasNorthPort ? 10 : PORT_STUB_LEN + 10));
            maxX = Math.max(maxX, el.x + el.w + (hasNorthPort ? 10 : PORT_STUB_LEN + 10));
            minY = Math.min(minY, el.y - (hasNorthPort ? PORT_STUB_LEN + 10 : 10));
            maxY = Math.max(maxY, el.y + el.h + (hasGnd ? GND_STUB_HEIGHT + GND_LINE_WIDTHS.length * GND_LINE_SPACING + 10 : 10));
        }
        for (const wire of layoutWires) {
            for (const seg of wire.segments) {
                minX = Math.min(minX, seg.x1, seg.x2);
                maxX = Math.max(maxX, seg.x1, seg.x2);
                minY = Math.min(minY, seg.y1, seg.y2);
                maxY = Math.max(maxY, seg.y1, seg.y2);
            }
        }

        const contentW = maxX - minX + padding * 2;
        const contentH = maxY - minY + padding * 2;
        const canvasW = canvas.clientWidth || 800;
        const canvasH = canvas.clientHeight || 600;
        zoomLevel = Math.min(canvasW / contentW, canvasH / contentH, 2.0);
        zoomLevel = Math.max(0.1, zoomLevel);
        panX = (canvasW - contentW * zoomLevel) / 2 - (minX - padding) * zoomLevel;
        panY = (canvasH - contentH * zoomLevel) / 2 - (minY - padding) * zoomLevel;
        render();
    }

    // ─────────── EVENTS ───────────

    function screenToWorld(sx, sy) {
        const rect = canvas.getBoundingClientRect();
        return { x: (sx - rect.left - panX) / zoomLevel, y: (sy - rect.top - panY) / zoomLevel };
    }

    canvas.addEventListener('mousemove', (e) => {
        const rect = canvas.getBoundingClientRect();
        tooltipScreenX = e.clientX - rect.left;
        tooltipScreenY = e.clientY - rect.top;
        const { x: mx, y: my } = screenToWorld(e.clientX, e.clientY);
        let foundItem = null, foundNet = null;
        for (const el of layoutElements) {
            if (mx >= el.x && mx <= el.x + el.w && my >= el.y && my <= el.y + el.h) {
                foundItem = el.name;
                break;
            }
        }
        if (!foundItem) {
            const hitDist = 6 / zoomLevel;
            for (const wire of layoutWires) {
                for (const seg of wire.segments) {
                    if (pointToSegmentDist(mx, my, seg.x1, seg.y1, seg.x2, seg.y2) < hitDist) {
                        foundNet = wire.netName;
                        break;
                    }
                }
                if (foundNet) break;
            }
        }
        let needsRedraw = false;
        if (foundItem !== hoveredItem) { hoveredItem = foundItem; canvas.style.cursor = foundItem ? 'pointer' : 'default'; needsRedraw = true; }
        if (foundNet !== hoveredNet) { hoveredNet = foundNet; if (!foundItem) canvas.style.cursor = foundNet ? 'crosshair' : 'default'; needsRedraw = true; }
        if (needsRedraw) render();
    });

    canvas.addEventListener('click', (e) => {
        if (hoveredItem) {
            const el = layoutElements.find(el => el.name === hoveredItem);
            if (vscodeApi) {
                if (el && el.type === 'instance' && el.entityType) vscodeApi.postMessage({ type: 'navigateToEntity', entityType: el.entityType });
                else if (el && el.line !== undefined && schematicData && schematicData.file_path) vscodeApi.postMessage({ type: 'navigateToLine', filePath: schematicData.file_path, line: el.line });
            }
        }
    });

    canvas.addEventListener('wheel', (e) => {
        e.preventDefault();
        if (e.ctrlKey || e.metaKey) {
            const rect = canvas.getBoundingClientRect();
            const mouseX = e.clientX - rect.left, mouseY = e.clientY - rect.top;
            const factor = Math.exp(-e.deltaY * 0.005);
            const newZoom = Math.max(0.05, Math.min(10, zoomLevel * factor));
            panX = mouseX - (mouseX - panX) * (newZoom / zoomLevel);
            panY = mouseY - (mouseY - panY) * (newZoom / zoomLevel);
            zoomLevel = newZoom;
        } else {
            panX -= e.deltaX;
            panY -= e.deltaY;
        }
        render();
    }, { passive: false });

    document.getElementById('btn-zoom-in').addEventListener('click', () => { zoomLevel = Math.min(10, zoomLevel * 1.3); render(); });
    document.getElementById('btn-zoom-out').addEventListener('click', () => { zoomLevel = Math.max(0.05, zoomLevel / 1.3); render(); });
    document.getElementById('btn-zoom-fit').addEventListener('click', () => { zoomToFit(); });

    function pointToSegmentDist(px, py, x1, y1, x2, y2) {
        const dx = x2 - x1, dy = y2 - y1;
        const lenSq = dx * dx + dy * dy;
        if (lenSq === 0) return Math.hypot(px - x1, py - y1);
        let t = ((px - x1) * dx + (py - y1) * dy) / lenSq;
        t = Math.max(0, Math.min(1, t));
        return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
    }

    // ─────────── DATA LOADING ───────────

    function loadSchematicData(data) {
        schematicData = data;
        entityNameEl.textContent = data.entity_name || 'No entity';
        clockSignals = new Set();
        resetSignals = new Set();
        for (const p of (data.ports || [])) {
            if (p.type === 'clock') clockSignals.add(p.name);
            if (p.type === 'reset') resetSignals.add(p.name);
        }
        const instCount = data.instances ? data.instances.filter(i => !isPowerGroundSymbol(i)).length : 0;
        const netCount = data.nets ? data.nets.length : 0;
        const portCount = data.ports ? data.ports.length : 0;
        const simStatus = data.simulation ? ' | DC sim: ✓' : '';
        statsEl.textContent = `${portCount} ports, ${instCount} components, ${netCount} nets${simStatus}`;

        // Convert power_nets array to a Set for efficient lookup
        if (data.simulation && Array.isArray(data.simulation.power_nets)) {
            data.simulation.power_nets = new Set(data.simulation.power_nets);
        }
        computeLayout(); zoomToFit();
    }

    window.addEventListener('message', (event) => {
        if (event.data.type === 'updateSchematic') loadSchematicData(event.data.data);
    });
    if (window.__BHDL_SCHEMATIC_DATA__) loadSchematicData(window.__BHDL_SCHEMATIC_DATA__);
    window.addEventListener('resize', resizeCanvas);
    resizeCanvas();
})();
