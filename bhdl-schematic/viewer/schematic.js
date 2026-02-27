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
                el.displayName = useRefDes ? el.refdes : (el.handleName || el.name);
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
    const ANNOTATION_PAD = 20;                 // extra padding around annotation text
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

    const STAGE_COLORS = {
        input_protection: '#FF9800', overvoltage_protection: '#FF9800',
        esd_protection: '#FF9800', overvoltage_clamp: '#FF9800',
        input_filtering: '#42A5F5', output_filtering: '#7E57C2',
        noise_filtering: '#42A5F5', anti_alias: '#42A5F5', emi_filtering: '#42A5F5',
        regulation: '#66BB6A',
        signal_buffering: '#26C6DA', level_shifting: '#26C6DA',
        precision_measurement: '#AB47BC', control_loop: '#AB47BC',
        current_limiting: '#FFA726',
        loading: '#8D6E63',
    };
    function getStageColor(el) {
        if (el.intent && STAGE_COLORS[el.intent]) return STAGE_COLORS[el.intent];
        if (el.stageName && STAGE_COLORS[el.stageName]) return STAGE_COLORS[el.stageName];
        if (el.category === 'regulator') return STAGE_COLORS.regulation;
        return null;
    }

    let clockSignals = new Set();
    let resetSignals = new Set();
    let layoutElements = [];
    let layoutWires = [];
    let layoutStageZones = [];
    let layoutPathBounds = [];
    let activeFlowSet = null;
    let activeFlowNets = null;

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

    /** Build inline parameter display string (value + optional bank count annotation). */
    function buildInlineParamStr(params) {
        if (!params || params.length === 0) return '';
        const valStr = params
            .filter(p => p[1] && INLINE_PARAM_KEYS.has(p[0]))
            .map(p => formatParamValue(p[1]))
            .join(', ');
        return valStr || '';
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

    function computeBoxSize(name, entityType, inPorts, outPorts, parameters, category, symbolHint) {
        if (category && isSymbolCategory(category)) {
            // If symbol hint says "triangle" body, use opamp symbol size
            if (symbolHint && symbolHint.body === 'triangle') {
                const s = SYMBOL_SIZES['opamp'];
                return { w: s.boundW, h: s.boundH };
            }
            const s = SYMBOL_SIZES[category];
            return { w: s.boundW, h: s.boundH };
        }

        // For IC boxes with symbol hints, count pins per side for accurate sizing
        if (symbolHint && Object.keys(symbolHint.pin_sides).length > 0) {
            let leftCount = 0, rightCount = 0, topCount = 0, bottomCount = 0;
            let maxLeftW = 0, maxRightW = 0, maxTopW = 0, maxBottomW = 0;
            for (const [pin, side] of Object.entries(symbolHint.pin_sides)) {
                const tw = measureTextWidth(pin, FONT_SIZE);
                if (side === 'left') { leftCount++; maxLeftW = Math.max(maxLeftW, tw); }
                else if (side === 'right') { rightCount++; maxRightW = Math.max(maxRightW, tw); }
                else if (side === 'top') { topCount++; maxTopW = Math.max(maxTopW, tw); }
                else if (side === 'bottom') { bottomCount++; maxBottomW = Math.max(maxBottomW, tw); }
            }
            // Account for group separator gaps
            const groupGaps = symbolHint.groups ? symbolHint.groups.length * 8 : 0;
            const vertPortCount = Math.max(leftCount, rightCount, 1);
            const paramLines = (parameters && parameters.length > 0) ? 1 : 0;
            const h = HEADER_HEIGHT + INSTANCE_PADDING * 2 + vertPortCount * PORT_SPACING + paramLines * PARAM_ROW_HEIGHT + groupGaps;
            const horizPortCount = Math.max(topCount, bottomCount, 0);
            const displayLabel = entityType || name;
            const nameW = measureTextWidth(displayLabel, FONT_SIZE + 1);
            const minWidthFromHPorts = horizPortCount > 0 ? horizPortCount * PORT_SPACING + 40 : 0;
            const w = Math.max(INSTANCE_BOX_MIN_WIDTH, maxLeftW + maxRightW + 40, nameW + 30, minWidthFromHPorts);
            return { w, h };
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
                    // Passive components (inductor, capacitor, resistor, diode) are never
                    // regulators, even if they happen to have IN/OUT pin names.
                    const passiveCats = ['inductor', 'capacitor', 'resistor', 'diode'];
                    if (passiveCats.includes(inst.category)) return false;
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
                else if (inst.expansion_role === 'shunt' || (inst.expansion_role && inst.expansion_role.startsWith('output_'))) group.shunt.push(name);
            }
        }

        const mainPathNames = new Set();
        let shuntNames = [];        // {name, junctionName, junctionSide}
        const decouplingNames = [];  // {name, junctionName, junctionSide}
        let branchNames = [];        // {name}

        // Track which instances are bank parents (have bank children referencing them)
        const bankParentNames = new Set();
        for (const [, inst] of instMap) {
            if (inst.bank_parent) bankParentNames.add(inst.bank_parent);
        }

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

            // ── 7c-b. Demote secondary regulator chains from shared power sources ──
            // When a power source (VIN) feeds multiple main-path regulators via
            // power pins, keep the longest-chain regulator on the main band and
            // demote the rest to parallel branches below the main path.

            // Count downstream regulators reachable from a starting regulator,
            // bridging through power source nodes (e.g., buck→buck_L→V5_BUCK→reg33).
            function regulatorCascadeDepth(startReg) {
                let depth = 0;
                const visited = new Set([startReg]);
                const queue = [startReg];
                while (queue.length > 0) {
                    const cur = queue.shift();
                    // Follow allForward edges (direct driver→sink connections)
                    for (const next of (allForward.get(cur) || [])) {
                        if (visited.has(next)) continue;
                        visited.add(next);
                        const inst = instMap.get(next);
                        if (inst && inst.category === 'regulator') {
                            depth++;
                            queue.push(next);
                        } else if (inst && inst.category === 'inductor') {
                            // Follow through inductors (buck expansion: buck→L→power net)
                            queue.push(next);
                        }
                    }
                    // Bridge through power source nodes: if cur is a sink on a
                    // power-source-driven net, follow to other sinks on that net
                    for (const pNet of processedNets) {
                        if (pNet.driver.type !== 'power_source') continue;
                        if (!pNet.sinks.some(s => s.name === cur)) continue;
                        for (const sink of pNet.sinks) {
                            if (visited.has(sink.name)) continue;
                            visited.add(sink.name);
                            const inst = instMap.get(sink.name);
                            if (inst && inst.category === 'regulator') {
                                depth++;
                                queue.push(sink.name);
                            } else if (inst && inst.category === 'inductor') {
                                queue.push(sink.name);
                            }
                        }
                    }
                }
                return depth;
            }

            for (const psNode of powerSourceNodes) {
                const psNet = processedNets.find(net =>
                    net.driver.type === 'power_source' && net.driver.name === psNode.id
                );
                if (!psNet) continue;

                const regSinks = psNet.sinks.filter(s => {
                    if (!mainPathNames.has(s.name)) return false;
                    const inst = instMap.get(s.name);
                    return inst && inst.category === 'regulator';
                }).map(s => s.name);

                if (regSinks.length <= 1) continue;

                // Score by regulator cascade depth (downstream regulators reachable)
                // Tiebreaker: earlier in mainPathOrder = more primary (BFS visit order)
                const scored = regSinks.map(name => {
                    const depth = regulatorCascadeDepth(name);
                    return { name, depth };
                });
                scored.sort((a, b) => {
                    if (b.depth !== a.depth) return b.depth - a.depth;
                    return mainPathOrder.indexOf(a.name) - mainPathOrder.indexOf(b.name);
                });
                const keepName = scored[0].name;

                for (let i = 1; i < scored.length; i++) {
                    const demotedName = scored[i].name;
                    mainPathNames.delete(demotedName);
                    const idx = mainPathOrder.indexOf(demotedName);
                    if (idx >= 0) mainPathOrder.splice(idx, 1);
                    // Junction is the power source node — branches are placed
                    // near the power source they fan out from, not near keepName.
                    parallelBranchJunctions.set(demotedName, psNode.id);
                    branchNames.push({
                        name: demotedName,
                        junctionName: psNode.id,
                        junctionSide: 'left',
                        isParallel: true
                    });
                }
            }

            // ── 7c-c. Cascade demotion for downstream nodes of power-source-demoted regulators ──
            // When a regulator is demoted (e.g., reg5aux), its downstream chain
            // (__pwr_V5_AUX__ → reg18 → ...) must also leave the main band.
            for (const [demotedName, keepName] of parallelBranchJunctions) {
                const inst = instMap.get(demotedName);
                if (!(inst && inst.category === 'regulator')) continue;

                const toVisit = [demotedName];
                const visited = new Set([demotedName]);
                while (toVisit.length > 0) {
                    const cur = toVisit.pop();
                    for (const next of (allForward.get(cur) || [])) {
                        if (visited.has(next)) continue;
                        visited.add(next);
                        // Demote if still on main band (components or power sources)
                        if (mainPathNames.has(next) && next !== keepName) {
                            mainPathNames.delete(next);
                            const idx = mainPathOrder.indexOf(next);
                            if (idx >= 0) mainPathOrder.splice(idx, 1);
                            // Cascade-demoted regulators become sub-heads in branch layout
                            const nextInst = instMap.get(next);
                            if (nextInst && nextInst.category === 'regulator') {
                                parallelBranchJunctions.set(next, demotedName);
                            }
                            branchNames.push({
                                name: next,
                                junctionName: keepName,
                                junctionSide: 'left',
                                isParallel: true
                            });
                        }
                        // Always continue walking — path may go through intermediate
                        // nodes (power sources) not in mainPathNames but leading to
                        // downstream main-path nodes.
                        toVisit.push(next);
                    }
                }
            }

            // ── 7c-d. Demote power source nodes of cascade-demoted regulators ──
            // allForward doesn't have edges from reg → __pwr_RAIL__ (power source
            // is the net driver, not the regulator). Explicitly find and demote the
            // output power source nodes so step 7e can move their sinks to branches.
            {
                const cascadeDemoted = new Set(branchNames.filter(b => {
                    const inst = instMap.get(b.name);
                    return inst && inst.category === 'regulator';
                }).map(b => b.name));
                for (const regName of cascadeDemoted) {
                    const regBranch = branchNames.find(b => b.name === regName);
                    if (!regBranch) continue;
                    // Find power source net-drivers where reg is a sink
                    for (const net of processedNets) {
                        if (net.driver.type !== 'power_source') continue;
                        if (!net.sinks.some(s => s.name === regName)) continue;
                        const driverName = net.driver.name;
                        // Only demote if still on main band (guard prevents double-demotion)
                        if (!mainPathNames.has(driverName)) continue;
                        mainPathNames.delete(driverName);
                        const idx = mainPathOrder.indexOf(driverName);
                        if (idx >= 0) mainPathOrder.splice(idx, 1);
                        branchNames.push({
                            name: driverName,
                            junctionName: regBranch.junctionName,
                            junctionSide: 'left',
                            isParallel: true
                        });
                    }
                }
            }

            // Move downstream shunts of demoted components into the branch group
            for (const [demotedName, keepName] of parallelBranchJunctions) {
                // Use the demoted component's own branch junction (e.g., reg18's
                // junction is __pwr_VIN__), not keepName which is the cascade parent
                // (e.g., reg5aux).  This ensures children end up in the same
                // top-level branch group as their regulator.
                const demotedBranch = branchNames.find(b => b.name === demotedName);
                const childJunction = demotedBranch ? demotedBranch.junctionName : keepName;
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
                            junctionName: childJunction,
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
                    // Don't demote regulators — they define power domains and belong on main path
                    if (inst && inst.category === 'regulator') continue;

                    // Check forward depth: does this sink reach other main-path nodes?
                    const fwd = allForward.get(s.name) || [];
                    const reachesMainPath = fwd.some(n => mainPathNames.has(n) && n !== s.name);
                    if (reachesMainPath) continue;

                    // Dead-end: demote to shunt
                    mainPathNames.delete(s.name);
                    const idx = mainPathOrder.indexOf(s.name);
                    if (idx >= 0) mainPathOrder.splice(idx, 1);

                    // Find the driver (junction point) for the shunt
                    const junctionName = net.driver.type === 'power_source'
                        ? net.driver.name
                        : (mainPathNames.has(net.driver.name) ? net.driver.name : null);

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

        // ── 7e. Propagate branch membership through net drivers ──
        // Components driven by branch members should also be in the branch group.
        // E.g., reg5aux (branch) drives c5a → c5a should be branch too.
        // Also fix undefined junctions on existing branch members.
        // Repeat until stable (handles chains like reg5aux → reg18 → c18).
        {
            const branchSet = new Set(branchNames.map(b => b.name));
            const branchJunctionFor = new Map(branchNames.map(b => [b.name, b.junctionName]));
            const chainChildren = new Set(shuntChainDown.values());
            let changed = true;
            while (changed) {
                changed = false;
                for (const net of processedNets) {
                    if (!branchSet.has(net.driver.name)) continue;
                    const driverJunction = branchJunctionFor.get(net.driver.name);
                    if (!driverJunction) continue;

                    for (const sink of net.sinks) {
                        // Skip shuntChainDown children — they'll be positioned
                        // by the post-branch stacking pass, not branch chain ordering
                        if (chainChildren.has(sink.name)) continue;

                        if (branchSet.has(sink.name)) {
                            // Already in branch; fix undefined junction
                            const existing = branchNames.find(b => b.name === sink.name);
                            if (existing && !existing.junctionName) {
                                existing.junctionName = driverJunction;
                                existing.junctionSide = 'left';
                                existing.isParallel = true;
                                branchJunctionFor.set(sink.name, driverJunction);
                                changed = true;
                            }
                            continue;
                        }

                        // Move from shuntNames
                        const si = shuntNames.findIndex(s => s.name === sink.name);
                        if (si >= 0) {
                            shuntNames.splice(si, 1);
                            branchNames.push({
                                name: sink.name,
                                junctionName: driverJunction,
                                junctionSide: 'left',
                                isParallel: true
                            });
                            branchSet.add(sink.name);
                            branchJunctionFor.set(sink.name, driverJunction);
                            changed = true;
                            continue;
                        }

                        // Move from decouplingNames
                        const di = decouplingNames.findIndex(d => d.name === sink.name);
                        if (di >= 0) {
                            decouplingNames.splice(di, 1);
                            branchNames.push({
                                name: sink.name,
                                junctionName: driverJunction,
                                junctionSide: 'left',
                                isParallel: true
                            });
                            branchSet.add(sink.name);
                            branchJunctionFor.set(sink.name, driverJunction);
                            changed = true;
                        }
                    }
                }
            }
        }

        // ── 7f. Rescue orphan chain children whose parents moved to branches ──
        // shuntChainDown links (e.g., r_led5a→led5a) are created in section 3b.
        // If the parent (r_led5a) was later moved to branches by 7e, the child
        // (led5a) is left as an orphan shunt with wrong junction because 7e
        // skips chainChildren. Move these orphans to branches under their parent.
        {
            const branchSet = new Set(branchNames.map(b => b.name));
            for (const [parent, child] of shuntChainDown) {
                if (!branchSet.has(parent)) continue;
                const ci = shuntNames.findIndex(s => s.name === child);
                if (ci < 0) continue;
                // Find parent's branch junction
                const parentBranch = branchNames.find(b => b.name === parent);
                const junction = parentBranch ? parentBranch.junctionName : undefined;
                shuntNames.splice(ci, 1);
                branchNames.push({
                    name: child,
                    junctionName: junction,
                    junctionSide: 'left',
                    isParallel: true
                });
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
        // The series child (e.g., inductor in a buck converter) is physically
        // in series between the parent's output and downstream consumers,
        // so all of the parent's non-series forward edges must be rerouted
        // through the series child:  parent → child → downstream.
        const seriesChildNames = new Set();
        for (const [parentName, group] of expansionGroups) {
            if (!mainBandNodes.has(parentName)) continue;
            for (const childName of group.series) {
                if (!mainBandNodes.has(childName)) continue;
                seriesChildNames.add(childName);
                // Ensure parent → series child edge
                if (!mbForward.get(parentName).includes(childName)) {
                    mbForward.get(parentName).push(childName);
                    mbInDegree.set(childName, mbInDegree.get(childName) + 1);
                }
                // Reroute: move non-series forward edges from parent to child.
                // e.g., buck → [buck_L, reg33] becomes buck → buck_L → reg33
                const parentFwd = mbForward.get(parentName);
                const childFwd = mbForward.get(childName);
                for (let i = parentFwd.length - 1; i >= 0; i--) {
                    const target = parentFwd[i];
                    if (target === childName) continue;     // keep parent → child
                    if (seriesChildNames.has(target)) continue; // other series children
                    parentFwd.splice(i, 1);
                    mbInDegree.set(target, mbInDegree.get(target) - 1);
                    if (!childFwd.includes(target)) {
                        childFwd.push(target);
                        mbInDegree.set(target, mbInDegree.get(target) + 1);
                    }
                }
            }
        }
        // Remove power-source → series-child edges (they invert the real flow)
        // and add the REVERSE edge (series-child → power-source) since the
        // child's output feeds that power net (e.g., inductor output → VOUT).
        for (const psNode of powerSourceNodes) {
            const fwd = mbForward.get(psNode.id);
            if (!fwd) continue;
            for (let i = fwd.length - 1; i >= 0; i--) {
                if (seriesChildNames.has(fwd[i])) {
                    const childName = fwd[i];
                    mbInDegree.set(childName, mbInDegree.get(childName) - 1);
                    fwd.splice(i, 1);
                    // Add reverse edge: series child → power source
                    if (!mbForward.get(childName).includes(psNode.id)) {
                        mbForward.get(childName).push(psNode.id);
                        mbInDegree.set(psNode.id, mbInDegree.get(psNode.id) + 1);
                    }
                }
            }
        }

        // Kahn's topological sort — DFS-like (LIFO) to follow chains before siblings.
        // This ensures parallel branches (e.g., buck chain vs reg5aux chain from VIN)
        // are laid out sequentially rather than interleaved.
        const mainBandOrder = [];
        const topoStack = [];
        for (const [n, deg] of mbInDegree) { if (deg === 0) topoStack.push(n); }
        while (topoStack.length > 0) {
            const cur = topoStack.pop();  // LIFO — follow chain before siblings
            mainBandOrder.push(cur);
            const ready = [];
            for (const next of (mbForward.get(cur) || [])) {
                const newDeg = mbInDegree.get(next) - 1;
                mbInDegree.set(next, newDeg);
                if (newDeg === 0) ready.push(next);
            }
            // Push in reverse order so the first forward neighbor ends up on
            // top of the stack and is processed next (DFS prefers first child).
            for (let i = ready.length - 1; i >= 0; i--) {
                topoStack.push(ready[i]);
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
                // Power sources take zero width in the main band — they render
                // as inline flags on the wire start, not as separate boxes.
                // The gap computation for shunt children will provide spacing.
                w = 0;
                h = 20;
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
                const size = computeBoxSize(nodeId, inst.entity_type, inP, outP, inst.parameters, inst.category, inst.symbol);
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
            if (inst && inst.expansion_parent && (inst.expansion_role === 'shunt' || (inst.expansion_role && inst.expansion_role.startsWith('output_')))) {
                const group = expansionGroups.get(inst.expansion_parent);
                if (group && group.series.length > 0) {
                    const seriesName = group.series[0];
                    const seriesInst = instMap.get(seriesName);
                    item.junctionName = seriesName;
                    // Default to right; override to left if shunt shares the
                    // series child's internal net (e.g., catch diode on SW node).
                    item.junctionSide = 'right';
                    if (seriesInst) {
                        // Determine the series child's "internal" (input-side) net:
                        // the signal net SHARED with the expansion parent.
                        // e.g., buck_sw is shared between buck(SW) and buck_L(pin2).
                        // The non-shared net (V5_BUCK) is the expansion output.
                        const parentInst2 = instMap.get(inst.expansion_parent);
                        const parentSignalNets = new Set();
                        if (parentInst2) {
                            for (const c of parentInst2.connections) {
                                if (!gndNetNames.has(c.signal)) parentSignalNets.add(c.signal);
                            }
                        }
                        const seriesInternalNets = new Set();
                        for (const c of seriesInst.connections) {
                            if (!gndNetNames.has(c.signal) && parentSignalNets.has(c.signal))
                                seriesInternalNets.add(c.signal);
                        }
                        const shuntNets = inst.connections
                            .filter(c => !gndNetNames.has(c.signal))
                            .map(c => c.signal);
                        if (shuntNets.some(n => seriesInternalNets.has(n))) {
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

        // Bank children inherit their parent's junction so siblings stay grouped
        {
            const itemByName = new Map();
            for (const item of [...shuntNames, ...decouplingNames]) itemByName.set(item.name, item);
            for (const item of [...shuntNames, ...decouplingNames]) {
                const inst = instMap.get(item.name);
                if (!inst || !inst.bank_parent) continue;
                const parentItem = itemByName.get(inst.bank_parent);
                if (parentItem && parentItem.junctionName) {
                    item.junctionName = parentItem.junctionName;
                    item.junctionSide = parentItem.junctionSide || 'right';
                    item.junctionNet = parentItem.junctionNet;
                }
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
                // Determine if shunt connects to the series child's internal net
                // (shared with expansion parent) or the expansion output net.
                // Internal net = shared with parent (e.g., buck_sw shared between
                // buck and buck_L). Output net = not shared (e.g., V5_BUCK).
                let onInternalNet = false;
                if (seriesInst && inst) {
                    const expParentInst = instMap.get(parentName);
                    const parentSignalNets = new Set();
                    if (expParentInst) {
                        for (const c of expParentInst.connections) {
                            if (!gndNetNames.has(c.signal)) parentSignalNets.add(c.signal);
                        }
                    }
                    const seriesInternalNets = new Set();
                    for (const c of seriesInst.connections) {
                        if (!gndNetNames.has(c.signal) && parentSignalNets.has(c.signal))
                            seriesInternalNets.add(c.signal);
                    }
                    const shuntNets = inst.connections
                        .filter(c => !gndNetNames.has(c.signal))
                        .map(c => c.signal);
                    onInternalNet = shuntNets.some(n => seriesInternalNets.has(n));
                }
                if (onInternalNet && parentName) {
                    // Internal-net shunt → junction at parent's right side
                    item.junctionName = parentName;
                    item.junctionSide = 'right';
                } else {
                    // Output-net shunt → junction at next node in mainBandOrder.
                    // Use mainBandOrder (topological sort) rather than mainPathOrder
                    // (BFS) because mainPathOrder can be in wrong order when multiple
                    // power sources seed the BFS in arbitrary net-list order.
                    // mainBandOrder is always correct and includes power source nodes.
                    const mbIdx = mainBandOrder.indexOf(seriesChild);
                    if (mbIdx >= 0 && mbIdx + 1 < mainBandOrder.length) {
                        item.junctionName = mainBandOrder[mbIdx + 1];
                        item.junctionSide = 'right';
                    } else {
                        // Last resort: virtual post-expansion junction
                        item.junctionName = '__post_expansion_' + parentName + '__';
                        item.junctionSide = 'right';
                        item._postExpansionParent = parentName;
                    }
                }
            }
        }

        // Also redirect non-expansion shunts away from expansion parent nodes,
        // but ONLY if the shunt is on the parent's INPUT net (e.g., input caps
        // that should be placed to the left, at the preceding power source node).
        // Shunts on the OUTPUT net (e.g., load resistors, LEDs) must stay at the
        // expansion parent — they're output loads, not input filtering.
        {
            const expParentSet = new Set(expansionGroups.keys());
            for (const item of [...shuntNames, ...decouplingNames, ...branchNames]) {
                if (!item.junctionName || !expParentSet.has(item.junctionName)) continue;
                const inst = instMap.get(item.name);
                if (inst && inst.expansion_parent) continue; // expansion children stay
                const parentName = item.junctionName;
                const parentInst = instMap.get(parentName);
                // Only redirect if shunt connects to the parent's input net.
                // Use pin_direction (from entity pin declaration), NOT direction
                // (which has a different meaning and may be 'in' for all ports).
                let onInputNet = false;
                if (parentInst && inst) {
                    const parentInputNets = new Set();
                    for (const c of parentInst.connections) {
                        if (c.pin_direction === 'in')
                            parentInputNets.add(c.signal);
                    }
                    const shuntNets = inst.connections
                        .filter(c => !gndNetNames.has(c.signal))
                        .map(c => c.signal);
                    onInputNet = shuntNets.some(n => parentInputNets.has(n));
                }
                if (!onInputNet) continue;
                // Find the previous node in mainBandOrder → redirect left
                const mbIdx = mainBandOrder.indexOf(parentName);
                if (mbIdx > 0) {
                    item.junctionName = mainBandOrder[mbIdx - 1];
                    item.junctionSide = 'right';
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

        // Group shunts/decoupling by (junctionName, junctionSide) for gap computation.
        // Exclude shuntChainDown children — they'll be positioned by the chain stacking
        // pass (line ~1803), not by boustrophedon placement.
        const chainChildSet = new Set(shuntChainDown.values());
        const verticalDropItems = [...shuntNames, ...decouplingNames].filter(
            item => !chainChildSet.has(item.name)
        );
        const dropGroups = new Map();
        const shuntGroupSide = new Map(); // itemName → 'left' | 'right'
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

        // ── Stage-order sort: within each group, sort by stage_order (from staged power flow).
        // Components with stage_order come first (sorted numerically), then those without
        // retain their original order.
        const stageOrderSort = (a, b) => {
            const instA = instMap.get(a.name);
            const instB = instMap.get(b.name);
            const soA = instA && instA.stage_order != null ? instA.stage_order : Infinity;
            const soB = instB && instB.stage_order != null ? instB.stage_order : Infinity;
            if (soA !== soB) return soA - soB;
            return 0; // preserve original order for equal/unset stage_order
        };
        for (const [, group] of dropGroups) {
            group.items.sort(stageOrderSort);
        }
        for (const [, group] of branchGroups) {
            group.items.sort(stageOrderSort);
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
                // Use the handle name (with _1 suffix for bank parents) since that's the default display
                let labelForLayout = itemName;
                if (bankParentNames.has(itemName)) labelForLayout = itemName + '_1';
                const nameW = measureTextWidth(labelForLayout, FONT_SIZE - 1);
                const paramStr = buildInlineParamStr(inst.parameters);
                const valW = (paramStr && inst.category !== 'resistor') ? measureTextWidth(paramStr, FONT_SIZE - 2) : 0;
                const side = shuntGroupSide.get(itemName) || 'left';
                if (side === 'right') {
                    // Right-group: name RIGHT, value LEFT
                    rightOverhang = Math.max(rightOverhang, nameW + 4);
                    if (valW > 0) leftOverhang = Math.max(leftOverhang, valW + 4);
                } else {
                    // Left-group (default): name LEFT, value RIGHT
                    leftOverhang = Math.max(leftOverhang, nameW + 4);
                    if (valW > 0) rightOverhang = Math.max(rightOverhang, valW + 4);
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

        // Compute bounding box for a layout group (shunt or branch) covering
        // ALL visual elements: component boxes, GND stubs, chain children, labels.
        // Returns {name, type, junctionName, side, x, y, w, h} or null if empty.
        function computeGroupBounds(group, groupKey, groupType) {
            const PADDING = 5;
            const gndStubTotal = GND_STUB_HEIGHT + GND_LINE_SPACING * GND_LINE_WIDTHS.length;
            let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;

            function expandItem(itemName) {
                const pos = positions.get(itemName);
                if (!pos || (pos.w === 0 && pos.h === 0)) return; // skip virtual nodes
                // Component box
                minX = Math.min(minX, pos.x);
                minY = Math.min(minY, pos.y);
                maxX = Math.max(maxX, pos.x + pos.w);
                // GND visual zone below component
                const gndBottom = Math.max(pos.y + pos.h, pos.gndTargetY || pos.y + pos.h) + gndStubTotal;
                maxY = Math.max(maxY, gndBottom);
                // Label overhang
                const oh = shuntItemOverhang(itemName);
                minX = Math.min(minX, pos.x - oh.left);
                maxX = Math.max(maxX, pos.x + pos.w + oh.right);
                // Walk shuntChainDown children
                let cur = itemName;
                while (shuntChainDown.has(cur)) {
                    cur = shuntChainDown.get(cur);
                    expandItem(cur);
                }
            }

            for (const item of group.items) {
                expandItem(item.name);
            }

            // Multi-row bbox override (if present, union with computed)
            if (group._bbox) {
                const bb = group._bbox;
                minX = Math.min(minX, bb.x);
                minY = Math.min(minY, bb.y);
                maxX = Math.max(maxX, bb.x + bb.w);
                maxY = Math.max(maxY, bb.y + bb.h);
            }

            if (minX === Infinity) return null;
            return {
                name: groupKey,
                type: groupType,
                junctionName: group.junctionName,
                side: group.side || 'right',
                x: minX - PADDING,
                y: minY - PADDING,
                w: (maxX - minX) + PADDING * 2,
                h: (maxY - minY) + PADDING * 2,
            };
        }

        // Multi-row shunt layout: max items per row before wrapping
        const MAX_SHUNT_PER_ROW = 5;

        function splitIntoRows(items, maxPerRow) {
            if (items.length <= maxPerRow) return [items];
            const rows = [];
            for (let i = 0; i < items.length; i += maxPerRow) {
                rows.push(items.slice(i, i + maxPerRow));
            }
            // Orphan control: if last row has just 1 item, merge into previous row
            if (rows.length > 1 && rows[rows.length - 1].length === 1) {
                const orphan = rows.pop()[0];
                rows[rows.length - 1].push(orphan);
            }
            return rows;
        }

        // Compute total width needed for a group of off-path items.
        // With multi-row support, returns the width of the widest row.
        function groupTotalWidth(items) {
            const rows = splitIntoRows(items, MAX_SHUNT_PER_ROW);
            let maxRowWidth = 0;
            for (const row of rows) {
                let rowTotal = 0;
                for (const item of row) {
                    const sz = offPathSizes.get(item.name);
                    if (sz) rowTotal += sz.w;
                }
                if (row.length > 1) {
                    for (let i = 0; i < row.length - 1; i++) {
                        const thisOH = shuntItemOverhang(row[i].name);
                        const nextOH = shuntItemOverhang(row[i + 1].name);
                        rowTotal += Math.max(MIN_ITEM_GAP_BASE, thisOH.right + nextOH.left + ANNOTATION_PAD);
                    }
                }
                if (row.length > 0) {
                    rowTotal += shuntItemOverhang(row[0].name).left;
                    rowTotal += shuntItemOverhang(row[row.length - 1].name).right;
                }
                maxRowWidth = Math.max(maxRowWidth, rowTotal);
            }
            return maxRowWidth;
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
        // Multi-row: groups with >MAX_SHUNT_PER_ROW items wrap into rows
        for (const [, group] of dropGroups) {
            const jPos = positions.get(group.junctionName);
            if (!jPos) continue;
            const items = group.items;
            const rows = splitIntoRows(items, MAX_SHUNT_PER_ROW);

            // Record which side of the junction each shunt is on,
            // so the renderer can place labels on the outward-facing side.
            for (const item of items) shuntGroupSide.set(item.name, group.side || 'right');

            const SHUNT_PORT_OFFSET = 20; // offset from port dot so T-junction is clear
            // Compute row stride: must accommodate GND stubs + clearance + matching drop-down.
            // Row 0's drop-down from rail to cap top = shuntY - (jPos.y + jPos.h/2).
            // For visual consistency, row 1+ should have the same drop from feed wire to cap top.
            const tallestH = Math.max(...items.map(it => (offPathSizes.get(it.name) || { h: 60 }).h));
            const gndSpace = GND_STUB_HEIGHT + GND_LINE_SPACING * 3;
            const row0Drop = shuntY - (jPos.y + jPos.h / 2); // ~118px
            const GND_CLEARANCE = 15;
            const ROW_STRIDE = rows.length > 1
                ? tallestH + gndSpace + GND_CLEARANCE + row0Drop
                : tallestH + gndSpace + 30;

            // Bus X position for multi-row vertical bus wire
            const busX = group.side === 'left'
                ? jPos.x - PORT_STUB_LEN
                : jPos.x + jPos.w + PORT_STUB_LEN;

            // Boustrophedon (alternating direction) placement:
            // Row 0: left→right, Row 1: right→left, Row 2: left→right, ...
            // Each row transition is a simple L-bend (no U-turns).
            const rowExtentsForGroup = [];

            for (let rowIdx = 0; rowIdx < rows.length; rowIdx++) {
                const row = rows[rowIdx];
                const rowY = shuntY + rowIdx * ROW_STRIDE;
                const itemSizes = row.map(it => offPathSizes.get(it.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 });
                const busYForRow = rowIdx > 0 ? rowY : undefined;

                // Determine direction: even rows follow group.side,
                // odd rows go in the opposite direction.
                const isReversedRow = rowIdx % 2 === 1;

                if (group.side === 'left') {
                    // Base direction: grow left. Reversed: grow right.
                    // Left-side boundary: reversed rows must not extend past
                    // the junction's left edge.
                    const junctionLeftX = jPos.x;
                    if (!isReversedRow) {
                        // Row 0: center last cap on port dot.
                        // Row 2+: align right edge with previous row's right edge.
                        let rx;
                        if (rowIdx === 0) {
                            const portDotX = jPos.x - PORT_STUB_LEN - SHUNT_PORT_OFFSET;
                            rx = portDotX + itemSizes[row.length - 1].w / 2;
                        } else {
                            rx = Math.min(junctionLeftX, rowExtentsForGroup[rowIdx - 1].maxX);
                        }
                        for (let i = row.length - 1; i >= 0; i--) {
                            const sz = itemSizes[i];
                            rx -= sz.w;
                            positions.set(row[i].name, { x: rx, y: rowY, w: sz.w, h: sz.h, _rowIdx: rowIdx, _busX: busX, _busY: busYForRow });
                            const thisOH = shuntItemOverhang(row[i].name);
                            const nextOH = i > 0 ? shuntItemOverhang(row[i - 1].name) : { left: 0, right: 0 };
                            rx -= Math.max(MIN_ITEM_GAP_BASE, thisOH.left + nextOH.right + ANNOTATION_PAD);
                        }
                    } else {
                        // Reversed: grow right from prev row's leftmost X
                        const prevExt = rowExtentsForGroup[rowIdx - 1];
                        let lx = prevExt ? prevExt.minX : jPos.x - PORT_STUB_LEN - SHUNT_PORT_OFFSET;
                        for (let i = 0; i < row.length; i++) {
                            const sz = itemSizes[i];
                            positions.set(row[i].name, { x: lx, y: rowY, w: sz.w, h: sz.h, _rowIdx: rowIdx, _busX: busX, _busY: busYForRow });
                            const thisOH = shuntItemOverhang(row[i].name);
                            const nextOH = i + 1 < row.length ? shuntItemOverhang(row[i + 1].name) : { left: 0, right: 0 };
                            lx += sz.w + Math.max(MIN_ITEM_GAP_BASE, thisOH.right + nextOH.left + ANNOTATION_PAD);
                        }
                        // Clamp: if the reversed row extends past the junction,
                        // shift the entire row left so items stay on the input side.
                        let rowMaxX = -Infinity;
                        for (const item of row) {
                            const p = positions.get(item.name);
                            if (p) rowMaxX = Math.max(rowMaxX, p.x + p.w);
                        }
                        if (rowMaxX > junctionLeftX) {
                            const shift = rowMaxX - junctionLeftX;
                            for (const item of row) {
                                const p = positions.get(item.name);
                                if (p) p.x -= shift;
                            }
                        }
                    }
                } else {
                    // Base direction: grow right. Reversed: grow left.
                    // Right-side boundary: reversed rows must not extend past
                    // the junction's right edge (output caps should stay right
                    // of the regulator they belong to).
                    const junctionRightX = jPos.x + jPos.w;
                    if (!isReversedRow) {
                        // Row 0, 2, 4...: grow right from port dot
                        // Row 0: center first cap on port dot.
                        // Row 2+: align left edge with previous row's left edge.
                        let lx;
                        if (rowIdx === 0) {
                            const portDotX = jPos.x + jPos.w + PORT_STUB_LEN + SHUNT_PORT_OFFSET;
                            lx = portDotX - itemSizes[0].w / 2;
                        } else {
                            lx = Math.max(junctionRightX, rowExtentsForGroup[rowIdx - 1].minX);
                        }
                        for (let i = 0; i < row.length; i++) {
                            const sz = itemSizes[i];
                            positions.set(row[i].name, { x: lx, y: rowY, w: sz.w, h: sz.h, _rowIdx: rowIdx, _busX: busX, _busY: busYForRow });
                            const thisOH = shuntItemOverhang(row[i].name);
                            const nextOH = i + 1 < row.length ? shuntItemOverhang(row[i + 1].name) : { left: 0, right: 0 };
                            lx += sz.w + Math.max(MIN_ITEM_GAP_BASE, thisOH.right + nextOH.left + ANNOTATION_PAD);
                        }
                    } else {
                        // Row 1, 3, 5...: grow left from prev row's rightmost edge
                        const prevExt = rowExtentsForGroup[rowIdx - 1];
                        let rx = prevExt ? prevExt.maxX : 0;
                        for (let i = row.length - 1; i >= 0; i--) {
                            const sz = itemSizes[i];
                            rx -= sz.w;
                            positions.set(row[i].name, { x: rx, y: rowY, w: sz.w, h: sz.h, _rowIdx: rowIdx, _busX: busX, _busY: busYForRow });
                            const thisOH = shuntItemOverhang(row[i].name);
                            const nextOH = i > 0 ? shuntItemOverhang(row[i - 1].name) : { left: 0, right: 0 };
                            rx -= Math.max(MIN_ITEM_GAP_BASE, thisOH.left + nextOH.right + ANNOTATION_PAD);
                        }
                        // Clamp: if the reversed row extends past the junction,
                        // shift the entire row right so items stay on the output side.
                        let rowMinX = Infinity;
                        for (const item of row) {
                            const p = positions.get(item.name);
                            if (p) rowMinX = Math.min(rowMinX, p.x);
                        }
                        if (rowMinX < junctionRightX) {
                            const shift = junctionRightX - rowMinX;
                            for (const item of row) {
                                const p = positions.get(item.name);
                                if (p) p.x += shift;
                            }
                        }
                    }
                }

                // Collect row extents
                let minX = Infinity, maxX = -Infinity;
                for (const item of row) {
                    const p = positions.get(item.name);
                    if (p) { minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x + p.w); }
                }
                rowExtentsForGroup.push({ minX, maxX, rowY });
            }

            // Store row info for wire routing (vertical bus between rows)
            if (rows.length > 1) {
                group._rows = rows;
                group._rowStride = ROW_STRIDE;
                group._row0Drop = row0Drop;
                group._busX = busX;
                group._rowExtents = rowExtentsForGroup;
                // Junction Y = where the power rail wire is (for serpentine start)
                group._junctionY = jPos.y + jPos.h / 2;
                // Bounding box of the entire multi-row group (for obstacle avoidance)
                let bboxMinX = Infinity, bboxMaxX = -Infinity;
                for (const ext of rowExtentsForGroup) {
                    bboxMinX = Math.min(bboxMinX, ext.minX);
                    bboxMaxX = Math.max(bboxMaxX, ext.maxX);
                }
                const lastRowY = rowExtentsForGroup[rowExtentsForGroup.length - 1].rowY;
                const tallest = Math.max(...items.map(it => (offPathSizes.get(it.name) || { h: 60 }).h));
                group._bbox = {
                    x: bboxMinX - 20, // padding
                    y: shuntY - PORT_STUB_LEN - 10, // above first row port stubs
                    w: (bboxMaxX - bboxMinX) + 40,
                    h: (lastRowY + tallest + GND_STUB_HEIGHT + GND_LINE_SPACING * 3) - (shuntY - PORT_STUB_LEN - 10) + 20
                };
            }
        }

        // Resolve overlaps among all shunt/decoupling items.
        // Group by Y coordinate (row) so items in different rows don't push each other.
        {
            const allDrop = verticalDropItems.filter(i => positions.has(i.name));
            const byRow = new Map();
            for (const item of allDrop) {
                const y = Math.round(positions.get(item.name).y);
                if (!byRow.has(y)) byRow.set(y, []);
                byRow.get(y).push(item);
            }
            for (const [, rowItems] of byRow) {
                rowItems.sort((a, b) => positions.get(a.name).x - positions.get(b.name).x);
                for (let i = 1; i < rowItems.length; i++) {
                    const prev = positions.get(rowItems[i - 1].name);
                    const curr = positions.get(rowItems[i].name);
                    const prevOH = shuntItemOverhang(rowItems[i - 1].name);
                    const currOH = shuntItemOverhang(rowItems[i].name);
                    const effectiveGap = Math.max(MIN_ITEM_GAP_BASE, prevOH.right + currOH.left + ANNOTATION_PAD);
                    const minX = prev.x + prev.w + effectiveGap;
                    if (curr.x < minX) curr.x = minX;
                }
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
        // Multi-row group items get per-row GND alignment (within their group).
        // Non-multi-row items use Y-bucket alignment as before.
        {
            const shuntChainChildSet = new Set(shuntChainDown.values());
            const allDrop = verticalDropItems.filter(i => positions.has(i.name));

            // Separate multi-row items (have _rowIdx) from regular items
            const multiRowHandled = new Set();
            for (const [, group] of dropGroups) {
                if (!group._rows || group._rows.length <= 1) continue;
                // Per-row GND alignment within this multi-row group
                for (const row of group._rows) {
                    let maxBottomY = 0;
                    for (const item of row) {
                        const pos = positions.get(item.name);
                        if (!pos) continue;
                        maxBottomY = Math.max(maxBottomY, pos.y + pos.h);
                        multiRowHandled.add(item.name);
                    }
                    if (maxBottomY > 0) {
                        for (const item of row) {
                            const pos = positions.get(item.name);
                            if (pos) pos.gndTargetY = maxBottomY;
                        }
                    }
                }
            }

            // Regular items (not in multi-row groups): per-drop-group GND alignment
            // (Grouping by Y-bucket caused VIN shunts to get the same gndTargetY
            //  as V5_BUCK shunts with chain children, making GND bars cross through
            //  branch regulators below the VIN shunt row.)
            const regularDrop = allDrop.filter(i => !multiRowHandled.has(i.name));
            const itemToGroupKey = new Map();
            for (const [key, group] of dropGroups) {
                for (const item of group.items) {
                    itemToGroupKey.set(item.name, key);
                }
            }
            const byGroup = new Map();
            for (const item of regularDrop) {
                const key = itemToGroupKey.get(item.name) || '__ungrouped__';
                if (!byGroup.has(key)) byGroup.set(key, []);
                byGroup.get(key).push(item);
            }
            for (const [, groupItems] of byGroup) {
                let maxBottomY = 0;
                for (const item of groupItems) {
                    const pos = positions.get(item.name);
                    if (!pos) continue;
                    let bottomY = pos.y + pos.h;
                    if (shuntChainDown.has(item.name)) {
                        const childPos = positions.get(shuntChainDown.get(item.name));
                        if (childPos) bottomY = childPos.y + childPos.h;
                    }
                    if (shuntChainChildSet.has(item.name)) {
                        bottomY = pos.y + pos.h;
                    }
                    maxBottomY = Math.max(maxBottomY, bottomY);
                }
                if (maxBottomY > 0) {
                    for (const item of groupItems) {
                        const pos = positions.get(item.name);
                        if (pos) pos.gndTargetY = maxBottomY;
                        if (shuntChainDown.has(item.name)) {
                            const childPos = positions.get(shuntChainDown.get(item.name));
                            if (childPos) childPos.gndTargetY = maxBottomY;
                        }
                    }
                }
            }

            // Cross-group GND alignment: shunts in the same Y-row should share
            // the same gndTargetY so GND symbols form a clean horizontal line
            // across input caps, output caps, and other shunts in that row.
            // Only extend if there's no obstacle (branch regulator, etc.)
            // between the shunt and the new GND level.
            {
                const Y_BUCKET_TOL = 20; // px tolerance for "same row"
                const byYBucket = new Map();
                for (const item of regularDrop) {
                    const pos = positions.get(item.name);
                    if (!pos || pos.gndTargetY == null) continue;
                    const bucket = Math.round(pos.y / Y_BUCKET_TOL) * Y_BUCKET_TOL;
                    if (!byYBucket.has(bucket)) byYBucket.set(bucket, []);
                    byYBucket.get(bucket).push(item);
                }
                for (const [, bucketItems] of byYBucket) {
                    if (bucketItems.length < 2) continue;
                    const maxGndY = Math.max(...bucketItems.map(b => positions.get(b.name).gndTargetY));
                    for (const item of bucketItems) {
                        const pos = positions.get(item.name);
                        if (pos.gndTargetY >= maxGndY) continue;
                        // Check for obstacles: any non-shunt positioned component
                        // vertically between the current gndTargetY and the new maxGndY
                        // at this shunt's X location.
                        const shuntCx = pos.x + pos.w / 2;
                        let blocked = false;
                        for (const [oName, oPos] of positions) {
                            if (offPathNames.has(oName)) continue; // skip other shunts/branches
                            if (oName.startsWith('__')) continue;  // skip virtual nodes
                            // Is this component under the shunt's GND extension?
                            if (shuntCx >= oPos.x - 10 && shuntCx <= oPos.x + oPos.w + 10 &&
                                oPos.y + oPos.h > pos.gndTargetY && oPos.y < maxGndY) {
                                blocked = true;
                                break;
                            }
                        }
                        if (!blocked) {
                            pos.gndTargetY = maxGndY;
                            if (shuntChainDown.has(item.name)) {
                                const childPos = positions.get(shuntChainDown.get(item.name));
                                if (childPos) childPos.gndTargetY = maxGndY;
                            }
                        }
                    }
                }
            }
        }

        // ── 10b-bounds. Compute shunt group bounding boxes ──
        const shuntGroupBoundsMap = new Map();
        for (const [key, group] of dropGroups) {
            const bounds = computeGroupBounds(group, key, 'shunt_group');
            if (bounds) { shuntGroupBoundsMap.set(key, bounds); layoutPathBounds.push(bounds); }
        }

        // ── 10c. Place branches as horizontal chains ──
        const branchMultiRowData = []; // collect multi-row metadata for bus wire routing
        for (const [, group] of branchGroups) {
            const jPos = positions.get(group.junctionName);
            if (!jPos) continue;
            const ordered = orderBranchChain(group.items, processedNets);
            const isParallel = group.items.some(item => item.isParallel);

            let bx, by;
            if (isParallel) {
                // Parallel branch: start at junction's X (aligned with main-path sibling)
                bx = jPos.x;
                // Place below shunt group bounding boxes at this junction
                let maxYBelow = shuntY;
                for (const [, bounds] of shuntGroupBoundsMap) {
                    if (bounds.junctionName === group.junctionName) {
                        maxYBelow = Math.max(maxYBelow, bounds.y + bounds.h);
                    }
                }
                by = maxYBelow + 40; // 40px comfort gap between path boxes
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
                // Parallel sub-chains: each head (from parallelBranchJunctions) starts
                // a new row. Shunt children of each head are placed HORIZONTALLY in a
                // row below the head, like main-band shunts (not stacked vertically).

                // Group items by which head drives them (via allForward reachability)
                const heads = group.items.filter(i => parallelBranchJunctions.has(i.name));
                const nonHeads = group.items.filter(i => !parallelBranchJunctions.has(i.name));

                // No heads: place items horizontally below the junction node
                if (heads.length === 0) {
                    const shuntRowY = by;
                    let sx = bx;
                    for (const item of nonHeads) {
                        const sz = offPathSizes.get(item.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                        positions.set(item.name, { x: sx, y: shuntRowY, w: sz.w, h: sz.h });
                        const thisOH = shuntItemOverhang(item.name);
                        sx += sz.w + Math.max(MIN_ITEM_GAP_BASE, thisOH.right + ANNOTATION_PAD);
                    }
                } else {

                // Build head→children mapping: BFS through allForward (transitive reachability)
                const headChildren = new Map(); // headName → [items]
                const itemOwner = new Map(); // itemName → headName
                for (const h of heads) {
                    headChildren.set(h.name, []);
                }
                const nonHeadNames = new Set(nonHeads.map(i => i.name));
                const headSet = new Set(heads.map(h => h.name));
                for (const h of heads) {
                    const queue = [h.name];
                    const seen = new Set([h.name]);
                    while (queue.length > 0) {
                        const cur = queue.shift();
                        for (const next of (allForward.get(cur) || [])) {
                            if (seen.has(next)) continue;
                            seen.add(next);
                            if (nonHeadNames.has(next) && !itemOwner.has(next)) {
                                itemOwner.set(next, h.name);
                                headChildren.get(h.name).push(nonHeads.find(i => i.name === next));
                            }
                            // Don't cross into other heads' territory
                            if (!headSet.has(next)) queue.push(next);
                        }
                    }
                }
                // Orphan items: assign to closest head (if any)
                for (const item of nonHeads) {
                    if (!itemOwner.has(item.name) && heads.length > 0) {
                        const lastHead = heads[heads.length - 1];
                        headChildren.get(lastHead.name).push(item);
                    }
                }

                // Order heads by topology (first head in processedNets order)
                const headOrder = [];
                // Simple: use the net-connectivity order from processedNets
                const visited = new Set();
                for (const h of heads) {
                    if (visited.has(h.name)) continue;
                    visited.add(h.name);
                    headOrder.push(h);
                    // If this head's forward chain contains another head, that head comes after
                    for (const fwd of (allForward.get(h.name) || [])) {
                        if (headSet.has(fwd) && !visited.has(fwd)) {
                            visited.add(fwd);
                            headOrder.push(heads.find(hh => hh.name === fwd));
                        }
                    }
                }
                // Add remaining heads
                for (const h of heads) {
                    if (!visited.has(h.name)) headOrder.push(h);
                }

                // Position heads HORIZONTALLY with children as vertical drops.
                // Children sorted: caps first (LEFT, output_filtering), then
                // loads after (RIGHT, loading) — following flow statement order.

                // Filter out shuntChainDown children — they get stacked below
                // their parent by the chain stacking pass, not placed in rows.
                const chainChildSet = new Set(shuntChainDown.values());
                for (const [hName, hChildren] of headChildren) {
                    headChildren.set(hName, hChildren.filter(c => !chainChildSet.has(c.name)));
                }

                // Normalize children heights and sort: caps LEFT, loads RIGHT
                let globalMaxChildH = 0;
                for (const head of headOrder) {
                    const children = headChildren.get(head.name) || [];
                    for (const c of children) {
                        const sz = offPathSizes.get(c.name);
                        if (sz) globalMaxChildH = Math.max(globalMaxChildH, sz.h);
                    }
                    // Sort: capacitors first (LEFT), everything else after (RIGHT)
                    children.sort((a, b) => {
                        const catA = instMap.get(a.name)?.category || '';
                        const catB = instMap.get(b.name)?.category || '';
                        return (catA === 'capacitor' ? 0 : 1) - (catB === 'capacitor' ? 0 : 1);
                    });
                }
                // Normalize all children to the global tallest height
                for (const head of headOrder) {
                    for (const c of (headChildren.get(head.name) || [])) {
                        const sz = offPathSizes.get(c.name);
                        if (sz) sz.h = globalMaxChildH;
                    }
                }

                let hx = bx;
                for (const head of headOrder) {
                    const headSz = offPathSizes.get(head.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                    positions.set(head.name, { x: hx, y: by, w: headSz.w, h: headSz.h });

                    const children = headChildren.get(head.name) || [];
                    if (children.length > 0) {
                        // Separate caps and loads — only caps form serpentine rows.
                        // Loads always go in row 0 after caps (individual wire routing).
                        const caps = children.filter(c => (instMap.get(c.name)?.category || '') === 'capacitor');
                        const loads = children.filter(c => (instMap.get(c.name)?.category || '') !== 'capacitor');

                        const capRows = caps.length > 0 ? splitIntoRows(caps, MAX_SHUNT_PER_ROW) : [];
                        const dropY = by + headSz.h + SHUNT_DROP;
                        const portX = hx + headSz.w + PORT_STUB_LEN;
                        const busX = portX;
                        const gndSpace = GND_STUB_HEIGHT + GND_LINE_SPACING * 3;
                        const GND_CLEARANCE = 15;
                        const rowDrop = dropY - (by + headSz.h / 2);
                        const ROW_STRIDE = capRows.length > 1
                            ? globalMaxChildH + gndSpace + GND_CLEARANCE + rowDrop
                            : globalMaxChildH + gndSpace + 30;

                        let maxRowRight = hx + headSz.w;
                        const rowExtents = [];

                        // Place caps in serpentine rows (with bus wire metadata)
                        for (let rowIdx = 0; rowIdx < capRows.length; rowIdx++) {
                            const row = capRows[rowIdx];
                            const rowY = dropY + rowIdx * ROW_STRIDE;
                            const isReversed = rowIdx % 2 === 1;
                            const busYForRow = rowIdx > 0 ? rowY : undefined;

                            if (!isReversed) {
                                let lx;
                                if (rowIdx === 0) {
                                    lx = portX - (offPathSizes.get(row[0].name) || { w: INSTANCE_BOX_MIN_WIDTH }).w / 2;
                                } else {
                                    lx = Math.max(hx + headSz.w, rowExtents[rowIdx - 1].minX);
                                }
                                for (let si = 0; si < row.length; si++) {
                                    const shItem = row[si];
                                    const sz = offPathSizes.get(shItem.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                                    positions.set(shItem.name, { x: lx, y: rowY, w: sz.w, h: sz.h, _rowIdx: rowIdx, _busX: busX, _busY: busYForRow });
                                    const thisOH = shuntItemOverhang(shItem.name);
                                    const nextOH = si + 1 < row.length ? shuntItemOverhang(row[si + 1].name) : { left: 0, right: 0 };
                                    lx += sz.w + Math.max(MIN_ITEM_GAP_BASE, thisOH.right + nextOH.left + ANNOTATION_PAD);
                                }
                            } else {
                                // Offset odd rows by half the previous row's inter-cap
                                // pitch so vertical stubs land in gaps between row 0 caps
                                // (not through cap bodies).
                                const prevExt = rowExtents[rowIdx - 1];
                                const prevRow = capRows[rowIdx - 1];
                                const prevPitch = prevRow.length > 1
                                    ? (prevExt.maxX - prevExt.minX) / (prevRow.length - 1)
                                    : 0;
                                let rx = prevExt.maxX - prevPitch / 2;
                                for (let si = row.length - 1; si >= 0; si--) {
                                    const shItem = row[si];
                                    const sz = offPathSizes.get(shItem.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                                    rx -= sz.w;
                                    positions.set(shItem.name, { x: rx, y: rowY, w: sz.w, h: sz.h, _rowIdx: rowIdx, _busX: busX, _busY: busYForRow });
                                    const thisOH = shuntItemOverhang(shItem.name);
                                    const nextOH = si > 0 ? shuntItemOverhang(row[si - 1].name) : { left: 0, right: 0 };
                                    rx -= Math.max(MIN_ITEM_GAP_BASE, thisOH.left + nextOH.right + ANNOTATION_PAD);
                                }
                                // Clamp: reversed rows must not extend past the
                                // head element's right edge (output caps stay
                                // right of the regulator that produces them).
                                const clampMinX = hx + headSz.w;
                                let branchRowMinX = Infinity;
                                for (const it of row) {
                                    const p = positions.get(it.name);
                                    if (p) branchRowMinX = Math.min(branchRowMinX, p.x);
                                }
                                if (branchRowMinX < clampMinX) {
                                    const shift = clampMinX - branchRowMinX;
                                    for (const it of row) {
                                        const p = positions.get(it.name);
                                        if (p) p.x += shift;
                                    }
                                }
                            }

                            let minX = Infinity, maxX = -Infinity;
                            for (const shItem of row) {
                                const p = positions.get(shItem.name);
                                if (p) { minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x + p.w); }
                            }
                            rowExtents.push({ minX, maxX, rowY });
                            maxRowRight = Math.max(maxRowRight, maxX);

                            // Align GND targets within this cap row
                            let maxBottomY = 0;
                            for (const shItem of row) {
                                const pos = positions.get(shItem.name);
                                if (pos) {
                                    let bottomY = pos.y + pos.h;
                                    let cur = shItem.name;
                                    while (shuntChainDown.has(cur)) {
                                        cur = shuntChainDown.get(cur);
                                        const cPos = positions.get(cur);
                                        if (cPos) bottomY = Math.max(bottomY, cPos.y + cPos.h);
                                    }
                                    maxBottomY = Math.max(maxBottomY, bottomY);
                                }
                            }
                            if (maxBottomY > 0) {
                                for (const shItem of row) {
                                    const pos = positions.get(shItem.name);
                                    if (pos) pos.gndTargetY = maxBottomY;
                                }
                            }
                        }

                        // Place loads in row 0 after last cap (no bus metadata — individual wires)
                        if (loads.length > 0) {
                            let lx;
                            if (rowExtents.length > 0 && capRows[0] && capRows[0].length > 0) {
                                const lastCapOH = shuntItemOverhang(capRows[0][capRows[0].length - 1].name);
                                const firstLoadOH = shuntItemOverhang(loads[0].name);
                                lx = rowExtents[0].maxX + Math.max(MIN_ITEM_GAP_BASE, lastCapOH.right + firstLoadOH.left + ANNOTATION_PAD);
                            } else {
                                lx = portX - (offPathSizes.get(loads[0].name) || { w: INSTANCE_BOX_MIN_WIDTH }).w / 2;
                            }
                            for (let li = 0; li < loads.length; li++) {
                                const loadItem = loads[li];
                                const sz = offPathSizes.get(loadItem.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                                positions.set(loadItem.name, { x: lx, y: dropY, w: sz.w, h: sz.h });
                                const thisOH = shuntItemOverhang(loadItem.name);
                                const nextOH = li + 1 < loads.length ? shuntItemOverhang(loads[li + 1].name) : { left: 0, right: 0 };
                                lx += sz.w + Math.max(MIN_ITEM_GAP_BASE, thisOH.right + nextOH.left + ANNOTATION_PAD);
                            }
                            // Update maxRowRight for loads
                            for (const loadItem of loads) {
                                const lp = positions.get(loadItem.name);
                                if (lp) maxRowRight = Math.max(maxRowRight, lp.x + lp.w);
                            }
                            // GND alignment for loads only (independent from caps).
                            // Caps keep their own per-row GND alignment from the
                            // boustrophedon loop — syncing with load chain children
                            // (e.g., led18 below r_led18) would extend cap GND stubs
                            // through the serpentine feed wire.
                            let loadMaxBottom = 0;
                            for (const loadItem of loads) {
                                const pos = positions.get(loadItem.name);
                                if (pos) {
                                    let bottomY = pos.y + pos.h;
                                    let cur = loadItem.name;
                                    while (shuntChainDown.has(cur)) {
                                        cur = shuntChainDown.get(cur);
                                        const cPos = positions.get(cur);
                                        if (cPos) bottomY = Math.max(bottomY, cPos.y + cPos.h);
                                    }
                                    loadMaxBottom = Math.max(loadMaxBottom, bottomY);
                                }
                            }
                            if (loadMaxBottom > 0) {
                                for (const loadItem of loads) {
                                    const pos = positions.get(loadItem.name);
                                    if (pos) pos.gndTargetY = loadMaxBottom;
                                }
                            }
                        }

                        // Store multi-row metadata for bus wire routing (cap-only rows)
                        if (capRows.length > 1) {
                            let bboxMinX = Infinity, bboxMaxX = -Infinity;
                            for (const ext of rowExtents) {
                                bboxMinX = Math.min(bboxMinX, ext.minX);
                                bboxMaxX = Math.max(bboxMaxX, ext.maxX);
                            }
                            // Include loads in bbox width
                            for (const loadItem of loads) {
                                const lp = positions.get(loadItem.name);
                                if (lp) { bboxMinX = Math.min(bboxMinX, lp.x); bboxMaxX = Math.max(bboxMaxX, lp.x + lp.w); }
                            }
                            const lastRowY = rowExtents[rowExtents.length - 1].rowY;
                            branchMultiRowData.push({
                                headName: head.name,
                                rows: capRows, // cap-only rows for bus wire routing
                                rowStride: ROW_STRIDE,
                                row0Drop: rowDrop,
                                busX,
                                rowExtents,
                                junctionY: by + headSz.h / 2,
                                bbox: {
                                    x: bboxMinX - 20,
                                    y: dropY - PORT_STUB_LEN - 10,
                                    w: (bboxMaxX - bboxMinX) + 40,
                                    h: (lastRowY + globalMaxChildH + gndSpace) - (dropY - PORT_STUB_LEN - 10) + 20
                                }
                            });
                        }

                        hx = maxRowRight + 60;
                    } else {
                        hx += headSz.w + 60;
                    }
                }
                } // end else (has heads)
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

        // ── 10c-bounds. Compute branch group bounding boxes ──
        for (const [key, group] of branchGroups) {
            const bounds = computeGroupBounds(group, key, 'branch_group');
            if (bounds) layoutPathBounds.push(bounds);
        }

        // ── 10c-fix. Push main-band right if branches extend into next element ──
        // After branch placement, some branch children (e.g., reg18's children)
        // may extend past the next main-band element (e.g., into buck expansion).
        // Detect this and shift main-band elements right to avoid overlap.
        {
            // For each branch group, find rightmost position of any branch child
            // (including chain children below branch items).
            for (const [, group] of branchGroups) {
                let branchRight = 0;
                for (const item of group.items) {
                    const pos = positions.get(item.name);
                    if (!pos) continue;
                    branchRight = Math.max(branchRight, pos.x + pos.w);
                    // Include chain children
                    let cur = item.name;
                    while (shuntChainDown.has(cur)) {
                        cur = shuntChainDown.get(cur);
                        const cPos = positions.get(cur);
                        if (cPos) branchRight = Math.max(branchRight, cPos.x + cPos.w);
                    }
                }
                if (branchRight === 0) continue;

                // Also account for annotation overhang of rightmost items
                for (const item of group.items) {
                    const pos = positions.get(item.name);
                    if (!pos) continue;
                    const oh = shuntItemOverhang(item.name);
                    branchRight = Math.max(branchRight, pos.x + pos.w + oh.right);
                }

                // Find the junction's position in mainBandOrder to locate next node
                const jIdx = mainBandOrder.indexOf(group.junctionName);
                if (jIdx < 0) continue;

                // Find the next main-band element (or power source) to the right
                const BRANCH_MAIN_PAD = 40;
                for (let ni = jIdx + 1; ni < mainBandOrder.length; ni++) {
                    const nextPos = positions.get(mainBandOrder[ni]);
                    if (!nextPos) continue;
                    // Also check for local power sources positioned before this node
                    let leftEdge = nextPos.x;
                    for (const ps of powerSourceNodes) {
                        const psPos = positions.get(ps.id);
                        if (psPos && psPos.x < nextPos.x && psPos.x > branchRight - BRANCH_MAIN_PAD) {
                            leftEdge = Math.min(leftEdge, psPos.x);
                        }
                    }

                    if (branchRight + BRANCH_MAIN_PAD > leftEdge) {
                        const shift = branchRight + BRANCH_MAIN_PAD - leftEdge;
                        // Shift this node and everything after it to the right
                        for (let si = ni; si < mainBandOrder.length; si++) {
                            const pos = positions.get(mainBandOrder[si]);
                            if (pos) pos.x += shift;
                        }
                        // Also shift power sources that are at or after this position
                        for (const ps of powerSourceNodes) {
                            const psPos = positions.get(ps.id);
                            if (psPos && psPos.x >= leftEdge) {
                                psPos.x += shift;
                            }
                        }
                        // Also shift off-path items attached to shifted nodes.
                        // Skip chain children (shuntChainDown values) — they will
                        // be shifted via their parent's chain walk below, preventing
                        // double-shifting when the child also has a matching junction.
                        const shiftedNodes = new Set(mainBandOrder.slice(ni));
                        const chainChildrenSet = new Set(shuntChainDown.values());
                        for (const item of [...shuntNames, ...decouplingNames]) {
                            if (chainChildrenSet.has(item.name)) continue;
                            if (shiftedNodes.has(item.junctionName)) {
                                const iPos = positions.get(item.name);
                                if (iPos) iPos.x += shift;
                                // Chain children too
                                let cur = item.name;
                                while (shuntChainDown.has(cur)) {
                                    cur = shuntChainDown.get(cur);
                                    const cPos = positions.get(cur);
                                    if (cPos) cPos.x += shift;
                                }
                            }
                        }
                    }
                    break; // only check next neighbor
                }
            }
        }

        // ── 10c-mainband-bounds. Compute main-band bounding box ──
        {
            let mbMinX = Infinity, mbMinY = Infinity, mbMaxX = -Infinity, mbMaxY = -Infinity;
            for (const name of mainBandOrder) {
                const pos = positions.get(name);
                if (!pos || (pos.w === 0 && pos.h === 0)) continue;
                mbMinX = Math.min(mbMinX, pos.x);
                mbMinY = Math.min(mbMinY, pos.y);
                mbMaxX = Math.max(mbMaxX, pos.x + pos.w + PORT_STUB_LEN);
                mbMaxY = Math.max(mbMaxY, pos.y + pos.h);
            }
            for (const ps of powerSourceNodes) {
                const pos = positions.get(ps.id);
                if (!pos) continue;
                mbMinX = Math.min(mbMinX, pos.x);
                mbMinY = Math.min(mbMinY, pos.y);
                mbMaxX = Math.max(mbMaxX, pos.x + pos.w);
                mbMaxY = Math.max(mbMaxY, pos.y + pos.h);
            }
            if (mbMinX !== Infinity) {
                layoutPathBounds.push({
                    name: '__main_band__',
                    type: 'main_band',
                    junctionName: null,
                    side: null,
                    x: mbMinX - 5,
                    y: mbMinY - 5,
                    w: (mbMaxX - mbMinX) + 10,
                    h: (mbMaxY - mbMinY) + 10,
                });
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
                            // Skip power source drivers — their position is symbolic
                            // (placed at the right end of the rail), not physical.
                            // Using them for flip detection gives wrong orientation.
                            if (net.driver.type === 'power_source') continue;
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
                // Re-center any chain children below this component
                if (shuntChainDown.has(name)) {
                    let cur = name;
                    while (shuntChainDown.has(cur)) {
                        const childName = shuntChainDown.get(cur);
                        const parentP = positions.get(cur);
                        const childP = positions.get(childName);
                        if (parentP && childP) {
                            const cx = parentP.x + parentP.w / 2;
                            childP.x = cx - childP.w / 2;
                        }
                        cur = childName;
                    }
                }
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

        // ── 10e. Re-stack shuntChainDown children of branch members ──
        // The initial stacking pass (before branch positioning) skips children
        // whose parents weren't positioned yet. Now that branches are placed and
        // overlap resolution has finalized parent positions, re-stack children
        // below their branch-member parents.
        {
            const branchNameSet = new Set(branchNames.map(b => b.name));
            for (const [parent, child] of shuntChainDown) {
                if (!branchNameSet.has(parent)) continue;
                const parentPos = positions.get(parent);
                if (!parentPos) continue;
                const childSz = offPathSizes.get(child) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                const cx = parentPos.x + parentPos.w / 2;
                positions.set(child, {
                    x: cx - childSz.w / 2,
                    y: parentPos.y + parentPos.h + SHUNT_DROP,
                    w: childSz.w,
                    h: childSz.h
                });
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

            const instInPorts = [], instOutPorts = [], instTopPorts = [], instBottomPorts = [];
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
                } else if (inst.symbol && Object.keys(inst.symbol.pin_sides).length > 0) {
                    // IC box with symbol hints — use pin_sides for side assignment
                    const isFlipped = flippedNames.has(name);
                    const pinSide = inst.symbol.pin_sides[c.port];
                    if (!pinSide) {
                        // Pin not in symbol definition — fall back to heuristic
                        if (r.has('in') && !seenIn.has(c.port)) {
                            seenIn.add(c.port);
                            instInPorts.push({ name: c.port, x: 0, y: 0, pinType: getPortPinType(name, c.port), isClock: clockSignals.has(c.signal), isReset: resetSignals.has(c.signal), _side: 'left' });
                        }
                        if (r.has('out') && !seenOut.has(c.port)) {
                            seenOut.add(c.port);
                            instOutPorts.push({ name: c.port, x: 0, y: 0, pinType: getPortPinType(name, c.port), _side: 'right' });
                        }
                    } else if (pinSide === 'left') {
                        if (!seenIn.has(c.port)) {
                            seenIn.add(c.port);
                            instInPorts.push({ name: c.port, x: 0, y: 0, pinType: getPortPinType(name, c.port), isClock: clockSignals.has(c.signal), isReset: resetSignals.has(c.signal), _side: 'left' });
                        }
                    } else if (pinSide === 'right') {
                        if (!seenOut.has(c.port)) {
                            seenOut.add(c.port);
                            instOutPorts.push({ name: c.port, x: 0, y: 0, pinType: getPortPinType(name, c.port), _side: 'right' });
                        }
                    } else if (pinSide === 'top') {
                        if (!seenIn.has(c.port) && !seenOut.has(c.port)) {
                            seenIn.add(c.port); // prevent duplicate
                            instTopPorts.push({ name: c.port, x: 0, y: 0, pinType: getPortPinType(name, c.port), _side: 'top' });
                        }
                    } else if (pinSide === 'bottom') {
                        if (!seenIn.has(c.port) && !seenOut.has(c.port)) {
                            seenOut.add(c.port); // prevent duplicate
                            instBottomPorts.push({ name: c.port, x: 0, y: 0, pinType: getPortPinType(name, c.port), _side: 'bottom' });
                        }
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
            // Fix 2-pin main-band symbols where both ports ended up as 'in'
            // (e.g. inductors that are sinks on both connected nets). Promote
            // the port whose net driver is topologically later (further right)
            // to 'out' so ports end up on opposite sides of the component.
            if (isSymbol && !isShuntLike && instInPorts.length >= 2 && instOutPorts.length === 0) {
                const myIdx = mainBandOrder.indexOf(name);
                let bestPromoteIdx = -1;
                let bestDriverIdx = -Infinity;
                for (let pi = 0; pi < instInPorts.length; pi++) {
                    const portName = instInPorts[pi].name;
                    for (const net of processedNets) {
                        const isSink = net.sinks.some(s => s.name === name && s.port === portName);
                        if (!isSink) continue;
                        const driverName = net.driver.name;
                        // Power sources are at the right end → treat as rightmost
                        const driverIdx = net.driver.type === 'power_source'
                            ? Infinity
                            : mainBandOrder.indexOf(driverName);
                        const effectiveIdx = driverIdx >= 0 ? driverIdx : -1;
                        if (effectiveIdx > bestDriverIdx) {
                            bestDriverIdx = effectiveIdx;
                            bestPromoteIdx = pi;
                        }
                    }
                }
                if (bestPromoteIdx >= 0) {
                    const promoted = instInPorts.splice(bestPromoteIdx, 1)[0];
                    const isFlipped2 = flippedNames.has(name);
                    promoted.x = isFlipped2 ? pos.x : pos.x + pos.w;
                    instOutPorts.push(promoted);
                    // Ensure remaining input port has correct position
                    for (const p of instInPorts) {
                        p.x = isFlipped2 ? pos.x + pos.w : pos.x;
                    }
                }
            }
            // Compute coordinates for symbol-hint ports (pushed with placeholder x:0, y:0)
            if (inst.symbol && Object.keys(inst.symbol.pin_sides).length > 0) {
                const isFlipped = flippedNames.has(name);
                // Left-side ports: spaced vertically on left (or right if flipped)
                let leftIdx = 0;
                for (const p of instInPorts) {
                    if (p._side === 'left' || !p._side) {
                        p.y = pos.y + HEADER_HEIGHT + INSTANCE_PADDING + (leftIdx + 0.5) * PORT_SPACING;
                        p.x = isFlipped ? pos.x + pos.w : pos.x;
                        leftIdx++;
                    }
                }
                // Right-side ports: spaced vertically on right (or left if flipped)
                let rightIdx = 0;
                for (const p of instOutPorts) {
                    if (p._side === 'right' || !p._side) {
                        p.y = pos.y + HEADER_HEIGHT + INSTANCE_PADDING + (rightIdx + 0.5) * PORT_SPACING;
                        p.x = isFlipped ? pos.x : pos.x + pos.w;
                        rightIdx++;
                    }
                }
                // Top ports: spaced horizontally along the top edge
                for (let i = 0; i < instTopPorts.length; i++) {
                    const p = instTopPorts[i];
                    p.x = pos.x + (pos.w / (instTopPorts.length + 1)) * (i + 1);
                    p.y = pos.y;
                }
                // Bottom ports: spaced horizontally along the bottom edge
                for (let i = 0; i < instBottomPorts.length; i++) {
                    const p = instBottomPorts[i];
                    p.x = pos.x + (pos.w / (instBottomPorts.length + 1)) * (i + 1);
                    p.y = pos.y + pos.h;
                }
            }
            // Attach simulation annotations (current, power) if available.
            // build_simulation_annotations() unifies decomposed branches (e.g. regulators)
            // into a single entry per instance, so direct lookup is sufficient.
            const simCurrent = data.simulation?.instance_currents?.[name];
            const simPowerW = data.simulation?.instance_power?.[name];
            const refdes = inst.refdes || null;
            // Bank parent gets _1 suffix in display name (c_in → c_in_1)
            let handleName = name;
            if (bankParentNames.has(name)) handleName = name + '_1';
            const displayName = refdes && refdesChk?.checked ? refdes : handleName;
            const isExpShunt = inst.expansion_parent
                && inst.expansion_role && inst.expansion_role !== 'series';
            const shuntSide = shuntGroupSide.get(name) || null;
            layoutElements.push({ x: pos.x, y: pos.y, w: pos.w, h: pos.h, name, handleName, refdes, displayName, _isExpShunt: !!isExpShunt, shuntSide, type: 'instance', entityType: inst.entity_type, parameters: inst.parameters, category: inst.category, isShunt: isShuntLike, isFlipped: flippedNames.has(name), inputPorts: instInPorts, outputPorts: instOutPorts, topPorts: instTopPorts, bottomPorts: instBottomPorts, symbolHint: inst.symbol || null, gndStubs: gndStubsByInst.get(name) || [], pwrStubs: pwrStubsByInst.get(name) || [], pgStubs: [], line: inst.line, simCurrent, simPower: simPowerW, gndTargetY: pos.gndTargetY, _rowIdx: pos._rowIdx || 0, _busX: pos._busX, _busY: pos._busY, _busLeftX: pos._busLeftX, _connections: inst.connections, stageName: inst.stage_name || null, stageOrder: inst.stage_order != null ? inst.stage_order : null, stageRail: inst.stage_rail || null, intent: inst.intent || null, flowIds: inst.flow_ids || [] });
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
                if (el.topPorts && el.topPorts.length > 0) dbg.push(`    topPorts: [${el.topPorts.map(p=>p.name+'@'+p.x.toFixed(0)+','+p.y.toFixed(0)).join('; ')}]`);
                if (el.bottomPorts && el.bottomPorts.length > 0) dbg.push(`    bottomPorts: [${el.bottomPorts.map(p=>p.name+'@'+p.x.toFixed(0)+','+p.y.toFixed(0)).join('; ')}]`);
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
            // Check top/bottom ports first (symbol-hint IC)
            if (el.topPorts) {
                const tp = el.topPorts.find(p => p.name === portName);
                if (tp) return { x: tp.x, y: tp.y - PORT_STUB_LEN, dir: 0, _vertical: -1 };
            }
            if (el.bottomPorts) {
                const bp = el.bottomPorts.find(p => p.name === portName);
                if (bp) return { x: bp.x, y: bp.y + PORT_STUB_LEN, dir: 0, _vertical: 1 };
            }
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

        // Accumulated wire segments per net for closest-point routing.
        // As wires are routed, their segments are added here so subsequent
        // wires on the same net can snap to the nearest existing segment
        // instead of routing all the way back to the driver.
        const netAccumSegs = new Map();

        for (const net of processedNets) {
            const driverElName = net.driver.type === 'power_source' ? net.driver.name
                : net.driver.type === 'entity_port' ? '__entity_in__'
                : net.driver.name;
            const fromPos = findPort(driverElName, net.driver.port, 'out');
            if (!fromPos) continue;

            // Sort sinks: main-band sinks first (nearest-first to build trunk
            // incrementally from source), then shunt sinks. Main-band sinks
            // establish the horizontal trunk wire at port level; subsequent
            // main-band sinks extend from the existing trunk via closest-point
            // routing. Shunt sinks then snap to the trunk with T-junctions.
            const sortedSinks = [...net.sinks].sort((a, b) => {
                const aName = a.type === 'entity_port' ? '__entity_out__' : a.name;
                const bName = b.type === 'entity_port' ? '__entity_out__' : b.name;
                const aEl = elByName.get(aName);
                const bEl = elByName.get(bName);
                const aShunt = aEl && aEl.isShunt ? 1 : 0;
                const bShunt = bEl && bEl.isShunt ? 1 : 0;
                // Main-band (non-shunt) first
                if (aShunt !== bShunt) return aShunt - bShunt;
                const aPos = findPort(aName, a.port, 'in');
                const bPos = findPort(bName, b.port, 'in');
                const aDist = aPos ? Math.hypot(aPos.x - fromPos.x, aPos.y - fromPos.y) : 0;
                const bDist = bPos ? Math.hypot(bPos.x - fromPos.x, bPos.y - fromPos.y) : 0;
                // Main-band: nearest-first (build trunk incrementally)
                // Shunt: farthest-first (doesn't matter much with closest-point routing)
                return aShunt === 0 ? aDist - bDist : bDist - aDist;
            });

            for (const sink of sortedSinks) {
                const sinkElName = sink.type === 'entity_port' ? '__entity_out__' : sink.name;
                const toPos = findPort(sinkElName, sink.port, 'in');
                if (!toPos) continue;

                // Skip long wires from power sources to distant main-band sinks
                // that would cross through intermediate components. Instead, add a
                // power stub at the sink (KiCad convention for shared power rails).
                if (net.driver.type === 'power_source') {
                    const driverIdx = mainBandOrder.indexOf(driverElName);
                    const sinkIdx = mainBandOrder.indexOf(sinkElName);
                    if (driverIdx >= 0 && sinkIdx >= 0 && sinkIdx > driverIdx) {
                        const netSinkNames = new Set(net.sinks.map(s => s.name));
                        let crossings = 0;
                        for (let i = driverIdx + 1; i < sinkIdx; i++) {
                            if (!netSinkNames.has(mainBandOrder[i])) crossings++;
                        }
                        if (crossings > 0) {
                            const sinkLayout = elByName.get(sinkElName);
                            const sinkInst = instMap.get(sinkElName);
                            const psNode = powerSourceNodes.find(ps => ps.id === driverElName);

                            // Regulators: create a local power source symbol to the
                            // left of the VIN pin with a short wire, identical to how
                            // the original power source feeds nearby regulators.
                            if (sinkInst && sinkInst.category === 'regulator' && sinkLayout && toPos) {
                                const label = psNode ? psNode.label : (net.name || 'VCC');
                                const voltage = psNode ? psNode.voltage : net.voltage;
                                // Size matches original power source nodes
                                const localPsW = measureTextWidth(label, FONT_SIZE) + 20;
                                const localPsH = 30;
                                const localPsX = toPos.x - PORT_STUB_LEN - localPsW;
                                const localPsY = toPos.y - localPsH / 2;
                                const localPsName = `__local_pwr_${net.name}_${sinkElName}__`;
                                const outPort = { name: 'out', x: localPsX + localPsW, y: toPos.y };
                                layoutElements.push({
                                    x: localPsX, y: localPsY, w: localPsW, h: localPsH,
                                    name: localPsName, type: 'power_source', label,
                                    inputPorts: [], outputPorts: [outPort],
                                    gndStubs: [], pgStubs: [], pwrStubs: []
                                });
                                elByName.set(localPsName, layoutElements[layoutElements.length - 1]);
                                // Short wire: local power source → regulator VIN
                                const localFrom = { x: outPort.x, y: outPort.y };
                                const localTo = { x: toPos.x, y: toPos.y };
                                layoutWires.push({
                                    from: localFrom, to: localTo,
                                    netName: net.name, isPower: true, width: 1,
                                    netClass: net.net_class || 'power',
                                    sinkElName, sourceElName: localPsName,
                                    driverIsPowerSource: true,
                                    segments: [{ x1: outPort.x, y1: outPort.y, x2: toPos.x, y2: toPos.y }]
                                });
                            } else if (sinkLayout) {
                                // Non-regulators: power stub flag on top of component
                                if (!sinkLayout.pwrStubs) sinkLayout.pwrStubs = [];
                                sinkLayout.pwrStubs.push({
                                    port: sink.port,
                                    netName: net.name,
                                    voltage: psNode ? psNode.voltage : net.voltage
                                });
                            }
                            continue;
                        }
                    }

                    // Distance-based: power source far from regulator → local power source
                    // when branch elements (other regulators) occupy the gap between them.
                    // Pure shunt gaps (cap banks on the same net) don't trigger this.
                    // Also: branch regulators (off main band) always get a local power
                    // source — their wire would cross through the shunt row.
                    const sinkInst2 = instMap.get(sinkElName);
                    const sinkLayout2 = elByName.get(sinkElName);
                    if (sinkInst2 && sinkInst2.category === 'regulator' && sinkLayout2 && toPos) {
                        const isBranchReg = !mainBandNodes.has(sinkElName);
                        const xDist = Math.abs(toPos.x - fromPos.x);
                        const minX2 = Math.min(fromPos.x, toPos.x);
                        const maxX2 = Math.max(fromPos.x, toPos.x);
                        const hasBranchBetween = branchNames.some(b => {
                            const pos = positions.get(b.name);
                            return pos && pos.x > minX2 && pos.x < maxX2;
                        });
                        if (isBranchReg || (xDist > 200 && hasBranchBetween)) {
                            const psNode = powerSourceNodes.find(ps => ps.id === driverElName);
                            const label = psNode ? psNode.label : (net.name || 'VCC');
                            const localPsW = measureTextWidth(label, FONT_SIZE) + 20;
                            const localPsH = 30;
                            const localPsX = toPos.x - PORT_STUB_LEN - localPsW;
                            const localPsY = toPos.y - localPsH / 2;
                            const localPsName = `__local_pwr_${net.name}_${sinkElName}__`;
                            const outPort = { name: 'out', x: localPsX + localPsW, y: toPos.y };
                            layoutElements.push({
                                x: localPsX, y: localPsY, w: localPsW, h: localPsH,
                                name: localPsName, type: 'power_source', label,
                                inputPorts: [], outputPorts: [outPort],
                                gndStubs: [], pgStubs: [], pwrStubs: []
                            });
                            elByName.set(localPsName, layoutElements[layoutElements.length - 1]);
                            layoutWires.push({
                                from: { x: outPort.x, y: outPort.y },
                                to: { x: toPos.x, y: toPos.y },
                                netName: net.name, isPower: true, width: 1,
                                netClass: net.net_class || 'power',
                                sinkElName, sourceElName: localPsName,
                                driverIsPowerSource: true,
                                segments: [{ x1: outPort.x, y1: outPort.y, x2: toPos.x, y2: toPos.y }]
                            });
                            continue;
                        }
                    }
                }

                const sinkEl = elByName.get(sinkElName);
                const isShuntWire = sinkEl && sinkEl.isShunt;
                const segments = [];

                // Multi-row shunt: row 1+ items are connected via the
                // L-bend bus wire — skip ALL individual wire routing.
                // Check this BEFORE any other routing logic.
                if (sinkEl && sinkEl._rowIdx > 0 && sinkEl._busY != null) {
                    continue; // L-bend bus handles this connection
                }

                // dir: +1 = wire extends right from dot, -1 = wire extends left
                const fromDir = fromPos.dir || 1;   // driver output default: rightward
                const toDir = toPos.dir || -1;       // sink input default: leftward

                // ── Closest-point routing ──
                // Find the nearest existing wire segment on this net and route
                // orthogonally from it. For main-band sinks this extends the
                // trunk incrementally; for shunt sinks it creates T-junctions.
                const existingSegs = netAccumSegs.get(net.name) || [];

                if (existingSegs.length > 0) {
                    // Find closest point on any existing segment to the sink
                    let bestDist = Infinity, bestCx = fromPos.x, bestCy = fromPos.y;
                    let trunkExtSeg = null;
                    for (const seg of existingSegs) {
                        const dx = seg.x2 - seg.x1, dy = seg.y2 - seg.y1;
                        const lenSq = dx * dx + dy * dy;
                        if (lenSq < 1) continue;
                        const t = Math.max(0, Math.min(1,
                            ((toPos.x - seg.x1) * dx + (toPos.y - seg.y1) * dy) / lenSq));
                        const cx = seg.x1 + t * dx, cy = seg.y1 + t * dy;
                        const dist = Math.hypot(toPos.x - cx, toPos.y - cy);
                        if (dist < bestDist) { bestDist = dist; bestCx = cx; bestCy = cy; trunkExtSeg = null; }
                        // For shunt/drop sinks: also consider extending a horizontal
                        // trunk segment to the target's X, then dropping vertically.
                        // This gives a clean T-junction instead of an L-route from
                        // the trunk endpoint.
                        const isHoriz = Math.abs(dy) < 2;
                        if (isHoriz && (isShuntWire || toPos.y > cy + 20)) {
                            const extDist = Math.abs(toPos.y - seg.y1);
                            if (extDist < bestDist) {
                                bestDist = extDist;
                                bestCx = toPos.x;
                                bestCy = seg.y1;
                                // Build trunk extension from nearest segment end
                                const segMinX = Math.min(seg.x1, seg.x2);
                                const segMaxX = Math.max(seg.x1, seg.x2);
                                if (toPos.x < segMinX) {
                                    trunkExtSeg = { x1: segMinX, y1: seg.y1, x2: toPos.x, y2: seg.y1 };
                                } else if (toPos.x > segMaxX) {
                                    trunkExtSeg = { x1: segMaxX, y1: seg.y1, x2: toPos.x, y2: seg.y1 };
                                } else {
                                    trunkExtSeg = null; // already within segment range
                                }
                            }
                        }
                    }
                    // Route orthogonally from closest/projected point to sink
                    if (trunkExtSeg) {
                        segments.push(trunkExtSeg);
                        junctionPoints.push({ x: trunkExtSeg.x1, y: trunkExtSeg.y1 });
                    }
                    junctionPoints.push({ x: bestCx, y: bestCy });
                    const dxAbs = Math.abs(bestCx - toPos.x);
                    const dyAbs = Math.abs(bestCy - toPos.y);
                    if (dxAbs < 2 && dyAbs < 2) {
                        // Already at destination (shouldn't happen, but guard)
                    } else if (dxAbs < 2) {
                        // Same X: single vertical segment
                        segments.push({ x1: bestCx, y1: bestCy, x2: toPos.x, y2: toPos.y });
                    } else if (dyAbs < 2) {
                        // Same Y: single horizontal segment
                        segments.push({ x1: bestCx, y1: bestCy, x2: toPos.x, y2: toPos.y });
                    } else {
                        // L-route from closest point: horizontal then vertical
                        segments.push({ x1: bestCx, y1: bestCy, x2: toPos.x, y2: bestCy });
                        segments.push({ x1: toPos.x, y1: bestCy, x2: toPos.x, y2: toPos.y });
                    }
                } else if (isShuntWire || (toPos.y > fromPos.y + 20 && toDir <= 0)) {
                    // First shunt wire on this net — no existing segments.
                    // L-route from driver: horizontal trunk, then vertical drop.
                    junctionPoints.push({ x: toPos.x, y: fromPos.y });
                    if (Math.abs(fromPos.x - toPos.x) > 2) {
                        segments.push({ x1: fromPos.x, y1: fromPos.y, x2: toPos.x, y2: fromPos.y });
                    }
                    segments.push({ x1: toPos.x, y1: fromPos.y, x2: toPos.x, y2: toPos.y });
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
                // Power source IDs use __pwr_<name>__ format; strip to match simulation keys.
                const driverSimKey = driverElName.startsWith('__pwr_') ? driverElName.slice(6, -2) : driverElName;
                const driverCurrent = data.simulation?.instance_currents?.[driverSimKey];
                const driverIsPowerSource = net.driver.type === 'power_source';
                layoutWires.push({ from: fromPos, to: toPos, sinkElName, segments, width: net.width || 1, netName: net.name, netClass: net.net_class || 'signal', isPower: isPowerNet, voltage, current, driverCurrent, driverIsPowerSource });

                // Accumulate segments for closest-point routing of later wires
                if (segments.length > 0) {
                    if (!netAccumSegs.has(net.name)) netAccumSegs.set(net.name, []);
                    netAccumSegs.get(net.name).push(...segments);
                }
            }
        }
        layoutElements._junctionPoints = junctionPoints;

        // Collect multi-row group bounding boxes for wire routing obstacle avoidance.
        // Recompute from actual final element positions (the early group._bbox is stale
        // after overlap resolution and position adjustments).
        const multiRowObstacles = [];
        const multiRowItemNames = new Set();
        const elByName2 = new Map();
        for (const el of layoutElements) elByName2.set(el.name, el);
        function recomputeMultiRowBBox(rows) {
            let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
            let count = 0;
            for (const row of rows) {
                for (const item of row) {
                    const el = elByName2.get(item.name);
                    if (!el || (el.w === 0 && el.h === 0)) continue;
                    count++;
                    minX = Math.min(minX, el.x);
                    maxX = Math.max(maxX, el.x + el.w);
                    minY = Math.min(minY, el.y);
                    // Include GND stubs in height
                    const gndBottom = el.gndTargetY != null
                        ? el.gndTargetY + GND_STUB_HEIGHT + GND_LINE_SPACING * GND_LINE_WIDTHS.length
                        : el.y + el.h;
                    maxY = Math.max(maxY, gndBottom);
                }
            }
            if (count === 0) return null;
            const PAD = 15;
            return { x: minX - PAD, y: minY - PORT_STUB_LEN - PAD, w: (maxX - minX) + 2 * PAD, h: (maxY - (minY - PORT_STUB_LEN - PAD)) + PAD };
        }
        for (const [, group] of dropGroups) {
            if (!group._rows || group._rows.length < 2) continue;
            const bbox = recomputeMultiRowBBox(group._rows);
            if (!bbox) continue;
            multiRowObstacles.push(bbox);
            for (const row of group._rows) {
                for (const item of row) multiRowItemNames.add(item.name);
            }
        }
        // Also include branch multi-row group obstacles
        for (const brData of branchMultiRowData) {
            if (!brData.rows || brData.rows.length < 2) continue;
            const bbox = recomputeMultiRowBBox(brData.rows);
            if (!bbox) continue;
            multiRowObstacles.push(bbox);
            for (const row of brData.rows) {
                for (const item of row) multiRowItemNames.add(item.name);
            }
        }

        // Post-process: reroute wire segments that cross through multi-row
        // cap bank bounding boxes. For any horizontal segment that falls within
        // a multi-row bbox, detour it below the bbox.
        if (multiRowObstacles.length > 0) {
            for (const wire of layoutWires) {
                // Skip wires TO multi-row items (they use L-bend bus)
                if (multiRowItemNames.has(wire.sinkElName)) continue;
                if (wire.segments.length === 0) continue;

                let modified = false;
                for (const obs of multiRowObstacles) {
                    // Check each segment for crossing
                    const newSegs = [];
                    for (const seg of wire.segments) {
                        const isHoriz = Math.abs(seg.y1 - seg.y2) < 2;
                        if (isHoriz) {
                            const segMinX = Math.min(seg.x1, seg.x2);
                            const segMaxX = Math.max(seg.x1, seg.x2);
                            const obsRight = obs.x + obs.w;
                            const obsBottom = obs.y + obs.h;
                            // Does this horizontal segment cross through the obstacle?
                            if (seg.y1 > obs.y && seg.y1 < obsBottom &&
                                segMaxX > obs.x && segMinX < obsRight) {
                                // Reroute: go down to below obstacle, across, then back up
                                const detourY = obsBottom + 15;
                                // Vertical down to detour level
                                newSegs.push({ x1: seg.x1, y1: seg.y1, x2: seg.x1, y2: detourY });
                                // Horizontal across below obstacle
                                newSegs.push({ x1: seg.x1, y1: detourY, x2: seg.x2, y2: detourY });
                                // Vertical back up to original level
                                newSegs.push({ x1: seg.x2, y1: detourY, x2: seg.x2, y2: seg.y2 });
                                modified = true;
                                continue;
                            }
                        }
                        newSegs.push(seg);
                    }
                    if (modified) {
                        wire.segments.length = 0;
                        wire.segments.push(...newSegs);
                        // Update wire to/from Y for endpoint rendering
                        break;
                    }
                }
            }
        }

        // Build per-net exclude sets for component avoidance: all elements
        // on the same net share a trunk and should not block each other's
        // wires. Without this, trunk extensions through series components
        // (e.g., inductors) get detoured above the main band.
        const netExcludeSets = new Map();
        for (const wire of layoutWires) {
            const net = wire.netName;
            if (!net) continue;
            if (!netExcludeSets.has(net)) netExcludeSets.set(net, new Set());
            const s = netExcludeSets.get(net);
            if (wire.sourceElName) s.add(wire.sourceElName);
            if (wire.sinkElName) {
                s.add(wire.sinkElName);
            }
        }

        // Post-process: reroute horizontal wire segments that pass through
        // main-band component bounding boxes. Detour above the obstacle.
        {
            const DETOUR_MARGIN = 12;
            for (const wire of layoutWires) {
                if (wire.segments.length === 0) continue;
                const netSet = netExcludeSets.get(wire.netName) || new Set();
                const excludeSet = new Set([...netSet, wire.sourceElName, wire.sinkElName].filter(Boolean));
                let modified = false;
                const newSegs = [];
                for (const seg of wire.segments) {
                    const isHoriz = Math.abs(seg.y1 - seg.y2) < 2;
                    if (!isHoriz) { newSegs.push(seg); continue; }
                    const segMinX = Math.min(seg.x1, seg.x2);
                    const segMaxX = Math.max(seg.x1, seg.x2);
                    let blocked = null;
                    for (const el of layoutElements) {
                        if (el.type !== 'instance') continue;
                        if (excludeSet.has(el.name)) continue;
                        const elRight = el.x + el.w;
                        const elBottom = el.y + el.h;
                        if (seg.y1 > el.y && seg.y1 < elBottom &&
                            segMaxX > el.x && segMinX < elRight) {
                            blocked = el;
                            break;
                        }
                    }
                    if (blocked) {
                        // Detour above the blocked component
                        const detourY = blocked.y - DETOUR_MARGIN;
                        newSegs.push({ x1: seg.x1, y1: seg.y1, x2: seg.x1, y2: detourY });
                        newSegs.push({ x1: seg.x1, y1: detourY, x2: seg.x2, y2: detourY });
                        newSegs.push({ x1: seg.x2, y1: detourY, x2: seg.x2, y2: seg.y2 });
                        modified = true;
                    } else {
                        newSegs.push(seg);
                    }
                }
                if (modified) {
                    wire.segments.length = 0;
                    wire.segments.push(...newSegs);
                }
            }
        }

        // Post-process: reroute vertical wire segments that pass through
        // non-endpoint component bounding boxes. Detour to the right of the
        // obstacle. When the segment's start point is inside the obstacle
        // (e.g., bus wire originating near a component), first go up to
        // clear the top of the obstacle before detouring right.
        {
            const DETOUR_MARGIN = 12;
            for (const wire of layoutWires) {
                if (wire.segments.length === 0) continue;
                const netSet2 = netExcludeSets.get(wire.netName) || new Set();
                const excludeSet = new Set([...netSet2, wire.sourceElName, wire.sinkElName].filter(Boolean));
                let modified = false;
                const newSegs = [];
                for (const seg of wire.segments) {
                    const isVert = Math.abs(seg.x1 - seg.x2) < 2;
                    if (!isVert) { newSegs.push(seg); continue; }
                    const segMinY = Math.min(seg.y1, seg.y2);
                    const segMaxY = Math.max(seg.y1, seg.y2);
                    let blocked = null;
                    for (const el of layoutElements) {
                        if (el.type !== 'instance') continue;
                        if (excludeSet.has(el.name)) continue;
                        const elRight = el.x + el.w;
                        const elBottom = el.y + el.h;
                        if (seg.x1 > el.x && seg.x1 < elRight &&
                            segMaxY > el.y && segMinY < elBottom) {
                            blocked = el;
                            break;
                        }
                    }
                    if (blocked) {
                        const detourX = blocked.x + blocked.w + DETOUR_MARGIN;
                        const goingDown = seg.y2 > seg.y1;
                        if (goingDown) {
                            // Segment goes downward through obstacle.
                            // Route: up to top of obstacle → right → down past bottom → left back
                            const clearTopY = blocked.y - DETOUR_MARGIN;
                            const clearBotY = Math.max(seg.y2, blocked.y + blocked.h + DETOUR_MARGIN);
                            if (seg.y1 > blocked.y) {
                                // Start is inside obstacle — go up first
                                newSegs.push({ x1: seg.x1, y1: seg.y1, x2: seg.x1, y2: clearTopY });
                            }
                            const startY = Math.min(seg.y1, clearTopY);
                            newSegs.push({ x1: seg.x1, y1: startY, x2: detourX, y2: startY });
                            newSegs.push({ x1: detourX, y1: startY, x2: detourX, y2: seg.y2 });
                            newSegs.push({ x1: detourX, y1: seg.y2, x2: seg.x2, y2: seg.y2 });
                        } else {
                            // Segment goes upward through obstacle.
                            const clearBotY = blocked.y + blocked.h + DETOUR_MARGIN;
                            if (seg.y1 < blocked.y + blocked.h) {
                                newSegs.push({ x1: seg.x1, y1: seg.y1, x2: seg.x1, y2: clearBotY });
                            }
                            const startY = Math.max(seg.y1, clearBotY);
                            newSegs.push({ x1: seg.x1, y1: startY, x2: detourX, y2: startY });
                            newSegs.push({ x1: detourX, y1: startY, x2: detourX, y2: seg.y2 });
                            newSegs.push({ x1: detourX, y1: seg.y2, x2: seg.x2, y2: seg.y2 });
                        }
                        modified = true;
                    } else {
                        newSegs.push(seg);
                    }
                }
                if (modified) {
                    wire.segments.length = 0;
                    wire.segments.push(...newSegs);
                }
            }
        }

        // Store multi-row serpentine bus wire data for rendering.
        // Serpentine routing: after the last cap in row N, wire goes down past
        // GND stubs, left back to first cap X, down a bit, then right feeding row N+1.
        const multiRowBusWires = [];
        // Helper: recompute row extents from actual element positions
        function recomputeRowExtents(rows) {
            const extents = [];
            for (const row of rows) {
                let minX = Infinity, maxX = -Infinity, rowY = 0;
                for (const item of row) {
                    const el = elByName2.get(item.name);
                    if (!el || (el.w === 0 && el.h === 0)) continue;
                    minX = Math.min(minX, el.x);
                    maxX = Math.max(maxX, el.x + el.w);
                    rowY = el.y; // all items in a row share Y
                }
                extents.push({ minX, maxX, rowY });
            }
            return extents;
        }
        for (const [, group] of dropGroups) {
            if (!group._rows || group._rows.length <= 1) continue;

            // Recompute row extents from actual final positions (early
            // _rowExtents are stale after overlap resolution).
            const rowExtents = recomputeRowExtents(group._rows);

            // L-bend bus wire data for boustrophedon layout.
            // Each row transition is a simple L-bend:
            //   Even→Odd: vertical down from right end, horizontal left (feed)
            //   Odd→Even: vertical down from left end, horizontal right (feed)
            const lbends = [];
            for (let ri = 0; ri < group._rows.length - 1; ri++) {
                const curr = rowExtents[ri];
                const next = rowExtents[ri + 1];
                const isEvenToOdd = ri % 2 === 0;
                // L-bend corner: past the END of both rows so the feed wire
                // covers all caps (next row may extend past current after clamping)
                const cornerX = isEvenToOdd ? Math.max(curr.maxX, next.maxX) : Math.min(curr.minX, next.minX);
                // Feed wire at same drop distance as row 0 (rail → cap top)
                // so per-cap stubs are visually consistent across rows
                const feedY = next.rowY - (group._row0Drop || 80);
                const cornerY = feedY;
                // Feed extends to center of the far-end cap (not the row edge)
                const nextRow = group._rows[ri + 1];
                const farCap = isEvenToOdd ? nextRow[0] : nextRow[nextRow.length - 1];
                const farCapPos = positions.get(farCap.name);
                const feedEndX = farCapPos ? farCapPos.x + farCapPos.w / 2 : (isEvenToOdd ? next.minX : next.maxX);
                lbends.push({ cornerX, cornerY, feedEndX, feedY, isEvenToOdd });
            }

            // Fix chain connectivity (same as branch groups above)
            for (let li = 1; li < lbends.length; li++) {
                const prevLb = lbends[li - 1];
                const lb = lbends[li];
                if (prevLb.isEvenToOdd) {
                    prevLb.feedEndX = Math.min(prevLb.feedEndX, lb.cornerX);
                } else {
                    prevLb.feedEndX = Math.max(prevLb.feedEndX, lb.cornerX);
                }
                lb.startY = prevLb.feedY;
            }

            // Find the net name from wires targeting items in this group
            let busNetName = null;
            const firstItemName = group._rows[0][0].name;
            for (const wire of layoutWires) {
                if (wire.sinkElName === firstItemName) { busNetName = wire.netName; break; }
            }

            multiRowBusWires.push({
                side: group.side,
                rowExtents,
                lbends,
                rows: group._rows,
                junctionY: group._junctionY,
                netName: busNetName
            });
        }
        // Build bus wires for branch multi-row groups (same L-bend pattern)
        // The vertical bus segment must route PAST the row edge (not through
        // a cap center) so the wire doesn't cut through row 0 caps. We place
        // the corner 20px past the row edge — this sits on the power wire
        // that continues from the head's VO to the loads further right.
        for (const brData of branchMultiRowData) {
            if (!brData.rows || brData.rows.length <= 1) continue;
            const lbends = [];
            // Recompute from actual final positions (stale after layout adjustments)
            const brRowExtents = recomputeRowExtents(brData.rows);

            // Find net name from wires targeting items in this group
            let busNetName = null;
            const firstItemName = brData.rows[0][0].name;
            for (const wire of layoutWires) {
                if (wire.sinkElName === firstItemName) { busNetName = wire.netName; break; }
            }

            // Collect all wire segments on this net for closest-point snapping
            const netSegments = [];
            if (busNetName) {
                for (const wire of layoutWires) {
                    if (wire.netName === busNetName) {
                        for (const seg of wire.segments) netSegments.push(seg);
                    }
                }
            }

            for (let ri = 0; ri < brData.rows.length - 1; ri++) {
                const next = brRowExtents[ri + 1];
                const isEvenToOdd = ri % 2 === 0;
                // L-bend corner: PAST the row edge so the vertical bus wire
                // doesn't pass through any cap body. +20px clears the cap edge.
                const BUS_WIRE_PAD = 20;
                const cornerX = isEvenToOdd
                    ? Math.max(brRowExtents[ri].maxX, next.maxX) + BUS_WIRE_PAD
                    : Math.min(brRowExtents[ri].minX, next.minX) - BUS_WIRE_PAD;
                const feedY = next.rowY - (brData.row0Drop || 80);
                const cornerY = feedY;
                const nextRow = brData.rows[ri + 1];
                const farCap = isEvenToOdd ? nextRow[0] : nextRow[nextRow.length - 1];
                const farCapPos = positions.get(farCap.name);
                const feedEndX = farCapPos ? farCapPos.x + farCapPos.w / 2 : (isEvenToOdd ? next.minX : next.maxX);
                lbends.push({ cornerX, cornerY, feedEndX, feedY, isEvenToOdd });
            }

            // Fix chain connectivity: each L-bend li>0 must connect to the
            // previous feed wire.  Two adjustments:
            // 1. Extend the previous feed wire to reach this L-bend's cornerX.
            // 2. Record startY = previous feedY so the vertical extends up to
            //    the previous feed wire (not just from the current row Y).
            for (let li = 1; li < lbends.length; li++) {
                const prevLb = lbends[li - 1];
                const lb = lbends[li];
                // Extend previous feed wire to reach this L-bend's corner
                if (prevLb.isEvenToOdd) {
                    // Previous feed goes left: extend further left if needed
                    prevLb.feedEndX = Math.min(prevLb.feedEndX, lb.cornerX);
                } else {
                    // Previous feed goes right: extend further right if needed
                    prevLb.feedEndX = Math.max(prevLb.feedEndX, lb.cornerX);
                }
                // Store startY so the renderer draws the vertical from
                // the previous feed wire, not from the current row Y
                lb.startY = prevLb.feedY;
            }

            // Snap junctionY to the closest point on existing net wire segments.
            // Shortest distance from a point to a line segment is orthogonal,
            // so the bus wire taps exactly onto the power wire.
            let junctionY = brData.junctionY; // fallback: head center
            if (netSegments.length > 0 && lbends.length > 0) {
                const px = lbends[0].cornerX;
                let bestDist = Infinity;
                for (const seg of netSegments) {
                    // Closest point on segment (seg.x1,seg.y1)-(seg.x2,seg.y2) to point (px, py=junctionY)
                    const dx = seg.x2 - seg.x1, dy = seg.y2 - seg.y1;
                    const lenSq = dx * dx + dy * dy;
                    if (lenSq < 1) continue; // degenerate segment
                    // Project point onto segment line, clamp to [0,1]
                    const t = Math.max(0, Math.min(1,
                        ((px - seg.x1) * dx + (junctionY - seg.y1) * dy) / lenSq));
                    const cx = seg.x1 + t * dx, cy = seg.y1 + t * dy;
                    const dist = Math.hypot(px - cx, junctionY - cy);
                    if (dist < bestDist) { bestDist = dist; junctionY = cy; }
                }
            }

            multiRowBusWires.push({
                side: 'right',
                rowExtents: brRowExtents,
                lbends,
                rows: brData.rows,
                junctionY,
                netName: busNetName
            });
        }
        layoutElements._multiRowBusWires = multiRowBusWires;
        layoutElements._multiRowObstacles = multiRowObstacles;
        layoutElements._multiRowItemNames = multiRowItemNames;

        // Build stage zone bounding regions for rendering
        buildStageZones();
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

    /** Build stage zone bounding regions from layoutElements with stage data. */
    function buildStageZones() {
        layoutStageZones = [];
        const groups = new Map(); // key: "rail|stageName" → { els, order, color }
        for (const el of layoutElements) {
            if (el.type !== 'instance' || !el.stageName || !el.stageRail) continue;
            const key = el.stageRail + '|' + el.stageName;
            if (!groups.has(key)) {
                const color = getStageColor(el) || '#888';
                groups.set(key, { els: [], order: el.stageOrder || 0, rail: el.stageRail, name: el.stageName, color });
            }
            groups.get(key).els.push(el);
        }
        const PAD = 18;
        for (const [, g] of groups) {
            let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
            for (const el of g.els) {
                minX = Math.min(minX, el.x);
                minY = Math.min(minY, el.y);
                maxX = Math.max(maxX, el.x + el.w);
                maxY = Math.max(maxY, el.y + el.h);
                // Include GND stubs in bounding box
                if (el.gndStubs) for (const s of el.gndStubs) {
                    maxY = Math.max(maxY, s.y + GND_STUB_HEIGHT + GND_LINE_SPACING * 3);
                }
            }
            layoutStageZones.push({
                x: minX - PAD, y: minY - PAD - 14,
                w: maxX - minX + PAD * 2, h: maxY - minY + PAD * 2 + 14,
                label: g.name.replace(/_/g, ' '),
                rail: g.rail, order: g.order, color: g.color,
            });
        }
        // Sort by order so lower stages draw first (left-to-right visual)
        layoutStageZones.sort((a, b) => a.order - b.order);
    }

    /** Draw stage zone backgrounds behind components. Called before expansion groups. */
    function drawStageZones() {
        if (layoutStageZones.length === 0) return;
        for (const zone of layoutStageZones) {
            // Filled background
            ctx.save();
            ctx.globalAlpha = 0.06;
            ctx.fillStyle = zone.color;
            ctx.beginPath();
            ctx.roundRect(zone.x, zone.y, zone.w, zone.h, 8);
            ctx.fill();
            // Dashed border
            ctx.globalAlpha = 0.2;
            ctx.strokeStyle = zone.color;
            ctx.lineWidth = 1;
            ctx.setLineDash([4, 4]);
            ctx.stroke();
            ctx.setLineDash([]);
            // Stage label at top-left
            ctx.globalAlpha = 0.5;
            ctx.fillStyle = zone.color;
            ctx.font = `${FONT_SIZE - 1}px monospace`;
            ctx.textAlign = 'left';
            ctx.textBaseline = 'bottom';
            ctx.fillText(zone.label, zone.x + 6, zone.y + 12);
            ctx.restore();
        }
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

        drawStageZones();
        drawExpansionGroups();
        drawWires();
        drawMultiRowBusWires();
        for (const el of layoutElements) {
            // Flow-path dimming: dim components not in the active flow
            if (activeFlowSet && el.type === 'instance') {
                ctx.globalAlpha = activeFlowSet.has(el.name) ? 1.0 : 0.15;
            }
            if (el.type === 'entity_in' || el.type === 'entity_out') drawEntityBox(el);
            else if (el.type === 'power_source') drawPowerRailFlag(el);
            else if (isSymbolCategory(el.category)) drawSymbolComponent(el);
            else drawInstanceBox(el);
            ctx.globalAlpha = 1.0;
        }
        // Draw GND and power stubs on top
        for (const el of layoutElements) {
            if (activeFlowSet && el.type === 'instance') {
                ctx.globalAlpha = activeFlowSet.has(el.name) ? 1.0 : 0.15;
            }
            if (el.gndStubs && el.gndStubs.length > 0) drawGndStubs(el);
            if (el.pwrStubs && el.pwrStubs.length > 0) drawPowerStubs(el);
            ctx.globalAlpha = 1.0;
        }

        ctx.restore();

        // Draw simulation tooltip and legend for hovered items (in screen space)
        drawSimTooltip();
        drawLegend();
    }

    function drawSimTooltip() {
        const sim = schematicData?.simulation;
        const lines = []; // { text, color? }

        if (hoveredItem) {
            const el = layoutElements.find(e => e.name === hoveredItem);
            if (el && el.type === 'instance') {
                // Show both user handle and refdes in hover
                const nameLabel = el.refdes ? el.name + '  [' + el.refdes + ']' : el.name;
                lines.push({ text: nameLabel + ' (' + (el.entityType || '') + ')' });
                if (sim) {
                    if (el.simCurrent != null) lines.push({ text: 'I = ' + formatCurrent(el.simCurrent) });
                    if (el.simPower != null) lines.push({ text: 'P = ' + formatPower(el.simPower) });
                }
                // Show non-inline params on hover
                if (el.parameters) {
                    for (const [k, v] of el.parameters) {
                        if (!INLINE_PARAM_KEYS.has(k) && v) lines.push({ text: k + ': ' + v });
                    }
                }
                // Stage position (colored by stage color)
                const sc = getStageColor(el);
                if (el.stageName) {
                    let label = 'Stage: ' + el.stageName.replace(/_/g, ' ');
                    const rail = schematicData.power_rails?.find(r => r.name === el.stageRail);
                    if (rail?.stages?.length) label += ' (' + (el.stageOrder + 1) + '/' + rail.stages.length + ' on ' + el.stageRail + ')';
                    lines.push({ text: label, color: sc });
                }
                // Intent with params (colored by stage color)
                if (el.intent) {
                    const fp = (schematicData.flow_paths || []).find(f => (el.flowIds || []).includes(f.id));
                    let str = 'Intent: ' + el.intent;
                    if (fp?.intent_params?.length) str += '(' + fp.intent_params.map(([k, v]) => k + ': ' + v).join(', ') + ')';
                    lines.push({ text: str, color: sc });
                }
            }
        } else if (hoveredNet && sim) {
            lines.push({ text: hoveredNet });
            const v = sim.net_voltages?.[hoveredNet];
            if (v != null) lines.push({ text: 'V = ' + formatVoltage(v) });
            const isPwr = sim.power_nets && (
                sim.power_nets instanceof Set ? sim.power_nets.has(hoveredNet) : !!sim.power_nets[hoveredNet]
            );
            lines.push({ text: isPwr ? 'Class: power' : 'Class: signal' });
        }

        if (lines.length === 0) return;

        const px = tooltipScreenX, py = tooltipScreenY;
        const padX = 8, padY = 5;
        ctx.font = `${FONT_SIZE - 1}px monospace`;
        let maxW = 0;
        for (const l of lines) maxW = Math.max(maxW, ctx.measureText(l.text).width);
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

        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        for (let i = 0; i < lines.length; i++) {
            ctx.fillStyle = lines[i].color || (i === 0 ? '#fff' : '#bbb');
            ctx.fillText(lines[i].text, bx + padX, by + padY + i * (FONT_SIZE + 2));
        }
    }

    let tooltipScreenX = 0, tooltipScreenY = 0;

    /** Draw a compact color legend in the bottom-left corner (screen space). */
    function drawLegend() {
        if (layoutStageZones.length === 0) return;
        // Collect unique (stageName, color) pairs sorted by order
        const seen = new Set();
        const entries = [];
        for (const el of layoutElements) {
            if (el.type !== 'instance' || !el.stageName) continue;
            if (seen.has(el.stageName)) continue;
            seen.add(el.stageName);
            const c = getStageColor(el);
            if (c) entries.push({ name: el.stageName.replace(/_/g, ' '), color: c, order: el.stageOrder || 0 });
        }
        if (entries.length === 0) return;
        entries.sort((a, b) => a.order - b.order);

        const padX = 10, padY = 8, dotR = 5, rowH = 18;
        const titleH = 16;
        ctx.font = `${FONT_SIZE - 1}px monospace`;
        let maxW = ctx.measureText('Stages').width;
        for (const e of entries) maxW = Math.max(maxW, ctx.measureText(e.name).width);
        const boxW = padX * 2 + dotR * 2 + 8 + maxW;
        const boxH = padY * 2 + titleH + entries.length * rowH;
        const bx = 12, by = canvas.clientHeight - boxH - 12;

        ctx.fillStyle = 'rgba(20,20,20,0.8)';
        ctx.strokeStyle = '#555';
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.roundRect(bx, by, boxW, boxH, 6);
        ctx.fill();
        ctx.stroke();

        // Title
        ctx.fillStyle = '#ccc';
        ctx.font = `bold ${FONT_SIZE - 1}px monospace`;
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText('Stages', bx + padX, by + padY);

        // Entries
        ctx.font = `${FONT_SIZE - 1}px monospace`;
        for (let i = 0; i < entries.length; i++) {
            const ey = by + padY + titleH + i * rowH;
            // Colored dot
            ctx.fillStyle = entries[i].color;
            ctx.beginPath();
            ctx.arc(bx + padX + dotR, ey + rowH / 2, dotR, 0, Math.PI * 2);
            ctx.fill();
            // Label
            ctx.fillStyle = '#bbb';
            ctx.textBaseline = 'middle';
            ctx.fillText(entries[i].name, bx + padX + dotR * 2 + 8, ey + rowH / 2);
        }
    }

    function drawPowerRailFlag(el) {
        const port = el.outputPorts[0];
        const px = port ? port.x : (el.x + el.w);
        const py = port ? port.y : (el.y + el.h / 2);

        // Wire connection point is at stub end (where findPort returns)
        const cx = px + PORT_STUB_LEN;

        // Horizontal bar (power flag symbol — like KiCad VCC)
        const barW = 14;
        ctx.strokeStyle = COLORS.powerSrcBorder;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(cx - barW / 2, py - 10);
        ctx.lineTo(cx + barW / 2, py - 10);
        ctx.stroke();

        // Vertical stub from bar down to wire level
        ctx.beginPath();
        ctx.moveTo(cx, py - 10);
        ctx.lineTo(cx, py);
        ctx.stroke();

        // Rail name + voltage above the bar
        ctx.fillStyle = COLORS.powerSrcText;
        ctx.font = `bold ${FONT_SIZE - 1}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText(el.label, cx, py - 12);

        // Junction dot at wire connection point
        ctx.fillStyle = COLORS.powerSrcBorder;
        ctx.beginPath();
        ctx.arc(cx, py, PORT_DOT_R, 0, Math.PI * 2);
        ctx.fill();
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
        const stageColor = getStageColor(el);
        const borderColor = isHovered ? COLORS.highlight : (stageColor || COLORS.instanceBorder);
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
        if (stageColor && !isHovered) {
            ctx.fillStyle = stageColor; ctx.globalAlpha = 0.15; ctx.fill(); ctx.globalAlpha = 1.0;
        }
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
            const paramStr = buildInlineParamStr(el.parameters);
            if (paramStr) ctx.fillText(paramStr, el.x + el.w / 2, el.y + HEADER_HEIGHT + 22);
        }

        const showLabels = shouldShowPortLabels(el.category);

        if (el.isShunt) {
            // NORTH port: wire drops down from above
            // For multi-row row 1+ items, the L-bend bus wire connects
            // directly to the cap top — skip port stubs to avoid double dots.
            if (!el._rowIdx || el._rowIdx === 0) {
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

        // Top ports (symbol hint) — stub goes upward
        if (el.topPorts) {
            for (const port of el.topPorts) {
                const pc = portColor(port);
                ctx.strokeStyle = pc;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(port.x, port.y - PORT_STUB_LEN);
                ctx.stroke();
                ctx.fillStyle = pc;
                ctx.beginPath();
                ctx.arc(port.x, port.y - PORT_STUB_LEN, PORT_DOT_R - 0.5, 0, Math.PI * 2);
                ctx.fill();
                if (showLabels) {
                    ctx.fillStyle = COLORS.text;
                    ctx.font = `${FONT_SIZE - 1}px monospace`;
                    ctx.textAlign = 'center';
                    ctx.textBaseline = 'bottom';
                    ctx.fillText(port.name, port.x, port.y - PORT_STUB_LEN - 3);
                }
            }
        }

        // Bottom ports (symbol hint) — stub goes downward
        if (el.bottomPorts) {
            for (const port of el.bottomPorts) {
                const pc = portColor(port);
                ctx.strokeStyle = pc;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(port.x, port.y + PORT_STUB_LEN);
                ctx.stroke();
                ctx.fillStyle = pc;
                ctx.beginPath();
                ctx.arc(port.x, port.y + PORT_STUB_LEN, PORT_DOT_R - 0.5, 0, Math.PI * 2);
                ctx.fill();
                if (showLabels) {
                    ctx.fillStyle = COLORS.text;
                    ctx.font = `${FONT_SIZE - 1}px monospace`;
                    ctx.textAlign = 'center';
                    ctx.textBaseline = 'top';
                    ctx.fillText(port.name, port.x, port.y + PORT_STUB_LEN + 3);
                }
            }
        }

        // Group separators (symbol hint) — dim lines between pin groups
        if (el.symbolHint && el.symbolHint.groups && el.symbolHint.groups.length > 0) {
            ctx.save();
            ctx.strokeStyle = COLORS.textDim;
            ctx.lineWidth = 0.5;
            ctx.setLineDash([2, 2]);
            // Track cumulative pin count per side to know where separator lines go
            const sidePinCounts = { left: 0, right: 0 };
            for (const group of el.symbolHint.groups) {
                const s = group.side;
                if (s !== 'left' && s !== 'right') continue;
                if (sidePinCounts[s] > 0) {
                    // Draw separator line
                    const sepY = el.y + HEADER_HEIGHT + INSTANCE_PADDING + sidePinCounts[s] * PORT_SPACING;
                    const x1 = s === 'left' ? el.x + 4 : el.x + el.w / 2;
                    const x2 = s === 'left' ? el.x + el.w / 2 : el.x + el.w - 4;
                    ctx.beginPath();
                    ctx.moveTo(x1, sepY);
                    ctx.lineTo(x2, sepY);
                    ctx.stroke();
                    // Group label
                    ctx.fillStyle = COLORS.textDim;
                    ctx.font = `${FONT_SIZE - 3}px monospace`;
                    ctx.textAlign = s === 'left' ? 'left' : 'right';
                    ctx.textBaseline = 'bottom';
                    const labelX = s === 'left' ? el.x + 6 : el.x + el.w - 6;
                    ctx.fillText(group.label, labelX, sepY - 1);
                }
                sidePinCounts[s] += group.pins.length;
            }
            ctx.setLineDash([]);
            ctx.restore();
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

        // Symbol stroke color — tinted by stage/intent when available
        const stageColor = getStageColor(el);
        const symbolColor = isHovered ? COLORS.highlight : (stageColor || '#c0c0c0');
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
        // For vertical shunt diodes, determine orientation from pin_direction:
        // if the top (input) port is the cathode (pin_direction "out"), the
        // symbol must be flipped so the bar is at the top (cathode up, anode down).
        let diodeFlip = isFlipped;
        if (isVertical && (cat === 'diode' || cat === 'protection')) {
            const topPort = el.inputPorts[0];
            if (topPort) {
                // Find the connection whose port matches the top port name
                const conn = (el._connections || []).find(c => c.port === topPort.name);
                if (conn && conn.pin_direction === 'out') diodeFlip = !diodeFlip;
            }
        }

        if (cat === 'resistor') {
            drawResistorSymbol(cx, cy, isVertical);
        } else if (cat === 'capacitor') {
            drawCapacitorSymbol(cx, cy, isVertical);
        } else if (cat === 'inductor') {
            drawInductorSymbol(cx, cy, isVertical);
        } else if (cat === 'diode') {
            const entityLower = (el.entityType || '').toLowerCase();
            const isLED = entityLower.startsWith('led');
            drawDiodeSymbol(cx, cy, isVertical, isLED, diodeFlip);
        } else if (cat === 'protection') {
            drawTVSDiodeSymbol(cx, cy, isVertical);
        } else if (cat === 'opamp') {
            drawOpAmpSymbol(cx, cy, isFlipped, el.h);
        }

        // Port stubs and dots
        if (el.isShunt) {
            // Shunt: NORTH port (input from above)
            // Skip for multi-row row 1+ items (L-bend bus handles connection)
            if (!el._rowIdx || el._rowIdx === 0) {
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
        const paramStr = buildInlineParamStr(el.parameters);
        const valueInside = cat === 'resistor' && paramStr;

        // For vertical shunts, place labels on the outward-facing side:
        //   Left-group shunts:  name LEFT  (right-aligned), value RIGHT (left-aligned)
        //   Right-group shunts: name RIGHT (left-aligned),  value LEFT  (right-aligned)
        // This keeps labels growing away from the junction, not toward neighbors.
        const nameOnRight = isVertical && el.shuntSide === 'right';

        ctx.fillStyle = COLORS.text;
        ctx.font = `bold ${FONT_SIZE - 1}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        const labelAboveY = isVertical ? cy : el.y - 2;
        let labelAboveX;
        if (isVertical) {
            ctx.textBaseline = 'middle';
            if (nameOnRight) {
                ctx.textAlign = 'left';
                labelAboveX = el.x + el.w + 4;
            } else {
                ctx.textAlign = 'right';
                labelAboveX = el.x - 4;
            }
        } else {
            labelAboveX = cx;
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
                // Value on the opposite side from the name
                if (nameOnRight) {
                    ctx.textAlign = 'right';
                    ctx.textBaseline = 'middle';
                    ctx.fillText(paramStr, el.x - 4, cy);
                } else {
                    ctx.textAlign = 'left';
                    ctx.textBaseline = 'middle';
                    ctx.fillText(paramStr, el.x + el.w + 4, cy);
                }
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
        const currentAnnotatedNets = new Set(); // deduplicate trunk current labels per net
        const internalNets = new Set(schematicData?.simulation?.internal_nets || []); // DC-equivalent internal nets — suppress annotations
        const wireElByName = new Map();
        for (const el of layoutElements) wireElByName.set(el.name, el);
        for (const wire of layoutWires) {
            // Flow-path dimming: dim wires not in the active flow
            if (activeFlowNets) {
                ctx.globalAlpha = activeFlowNets.has(wire.netName) ? 1.0 : 0.15;
            }
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

            // Skip annotations on internal DC-equivalent nets (e.g. buck_sw ≡ V5_BUCK).
            // The canonical net carries the annotations; internal nets would overlap.
            const isInternalNet = internalNets.has(wire.netName);

            // ── Voltage annotation ──
            // Show voltage once per net — the first wire to claim the net wins.
            const alreadyAnnotated = voltageAnnotatedNets.has(wire.netName);
            const showVoltage = wire.voltage != null && !alreadyAnnotated && !isInternalNet;

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
            if (isInternalNet) {
                // Skip — canonical net carries the annotation
            } else if (isShuntLikeWire) {
                // Trunk: driver's total current on horizontal segment (once per net)
                const trunkCurrent = wire.driverCurrent;
                const currentAlreadyAnnotated = currentAnnotatedNets.has(wire.netName);
                if (!currentAlreadyAnnotated && trunkCurrent != null && Math.abs(trunkCurrent) > 1e-3
                    && horizSeg && Math.abs(horizSeg.x2 - horizSeg.x1) > 30) {
                    currentAnnotatedNets.add(wire.netName);
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
            } else if (wire.current != null && Math.abs(wire.current) > 1e-3 && bestSeg && bestLen > 20
                       // Skip if this wire carries the full net current (≈ driverCurrent) —
                       // the shunt trunk annotation already shows it.
                       && !(wire.driverCurrent != null && Math.abs(Math.abs(wire.current) - Math.abs(wire.driverCurrent)) < 1e-3)) {
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
            ctx.globalAlpha = 1.0;
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

    /** Draw L-bend bus wires for boustrophedon multi-row shunt groups.
     *
     *  Layout (right-side group):
     *    ═══ power rail ═══ [Cap1] [Cap2] ... [Cap5]
     *                                             │ ← L-bend down
     *                  [Cap10] [Cap9] ... [Cap6] ←┘ ← feed right-to-left
     *                    │
     *                    └→ [Cap11] ...                ← next L-bend
     */
    function drawMultiRowBusWires() {
        const busWires = layoutElements._multiRowBusWires;
        if (!busWires || busWires.length === 0) return;
        ctx.setLineDash([]);

        for (const bus of busWires) {
            const isHighlighted = hoveredNet && bus.netName === hoveredNet;
            const wireColor = isHighlighted ? COLORS.wireHighlight : COLORS.wire;
            const dotColor = isHighlighted ? COLORS.wireHighlight : (COLORS.junctionDot || COLORS.wire);

            for (let li = 0; li < bus.lbends.length; li++) {
                const lb = bus.lbends[li];
                const currExt = bus.rowExtents[li];

                // Vertical start: tap from the power rail Y (for first bend)
                // or from the previous feed wire Y (for subsequent bends)
                const startY = lb.startY != null ? lb.startY
                    : li === 0 ? (bus.junctionY || currExt.rowY) : currExt.rowY;

                // Draw L-bend: vertical down + horizontal feed
                ctx.strokeStyle = wireColor;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(lb.cornerX, startY);      // start at end of current row
                ctx.lineTo(lb.cornerX, lb.cornerY);   // vertical down to next row Y
                ctx.lineTo(lb.feedEndX, lb.feedY);     // horizontal feed across next row
                ctx.stroke();

                // Junction dot at tap point
                ctx.fillStyle = dotColor;
                ctx.beginPath();
                ctx.arc(lb.cornerX, startY, 3, 0, Math.PI * 2);
                ctx.fill();

                // Per-cap vertical stubs from feed wire to each cap's top pin
                const nextRow = bus.rows[li + 1];
                if (nextRow) {
                    ctx.strokeStyle = wireColor;
                    ctx.lineWidth = 1.5;
                    for (const item of nextRow) {
                        const el = layoutElements.find(e => e.name === item.name);
                        if (!el) continue;
                        const stubX = el.x + el.w / 2;
                        // Stub from feed wire down to cap top
                        if (Math.abs(lb.feedY - el.y) > 2) {
                            ctx.beginPath();
                            ctx.moveTo(stubX, lb.feedY);
                            ctx.lineTo(stubX, el.y);
                            ctx.stroke();
                        }
                        // Junction dot on feed wire
                        ctx.fillStyle = dotColor;
                        ctx.beginPath();
                        ctx.arc(stubX, lb.feedY, 3, 0, Math.PI * 2);
                        ctx.fill();
                    }
                }
            }
        }
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
            // Also check multi-row bus wire segments
            if (!foundNet) {
                const busWires = layoutElements._multiRowBusWires || [];
                for (const bus of busWires) {
                    if (!bus.netName) continue;
                    for (let bli = 0; bli < bus.lbends.length; bli++) {
                        const lb = bus.lbends[bli];
                        const startY = lb.startY != null ? lb.startY
                            : bli === 0 ? (bus.junctionY || bus.rowExtents[0].rowY) : bus.rowExtents[bli].rowY;
                        // Vertical segment
                        if (pointToSegmentDist(mx, my, lb.cornerX, startY, lb.cornerX, lb.cornerY) < hitDist) { foundNet = bus.netName; break; }
                        // Horizontal feed
                        if (pointToSegmentDist(mx, my, lb.cornerX, lb.feedY, lb.feedEndX, lb.feedY) < hitDist) { foundNet = bus.netName; break; }
                    }
                    if (foundNet) break;
                    // Per-cap stubs
                    for (let ri = 1; ri < bus.rows.length; ri++) {
                        const lb = bus.lbends[ri - 1];
                        if (!lb) continue;
                        for (const item of bus.rows[ri]) {
                            const el = layoutElements.find(e => e.name === item.name);
                            if (!el) continue;
                            const stubX = el.x + el.w / 2;
                            if (pointToSegmentDist(mx, my, stubX, lb.feedY, stubX, el.y) < hitDist) { foundNet = bus.netName; break; }
                        }
                        if (foundNet) break;
                    }
                    if (foundNet) break;
                }
            }
        }
        let needsRedraw = false;
        if (foundItem !== hoveredItem) { hoveredItem = foundItem; canvas.style.cursor = foundItem ? 'pointer' : 'default'; needsRedraw = true; }
        if (foundNet !== hoveredNet) { hoveredNet = foundNet; if (!foundItem) canvas.style.cursor = foundNet ? 'crosshair' : 'default'; needsRedraw = true; }
        // Compute active flow set for flow-path highlighting
        let newFlowSet = null, newFlowNets = null;
        if (foundItem && schematicData?.flow_paths?.length) {
            const el = layoutElements.find(e => e.name === foundItem);
            if (el && el.flowIds && el.flowIds.length > 0) {
                newFlowSet = new Set();
                newFlowNets = new Set();
                for (const fp of schematicData.flow_paths) {
                    if (el.flowIds.includes(fp.id)) {
                        for (const c of fp.components) newFlowSet.add(c);
                        for (const n of fp.nets) newFlowNets.add(n);
                    }
                }
            }
        }
        const flowChanged = (newFlowSet !== activeFlowSet);
        activeFlowSet = newFlowSet;
        activeFlowNets = newFlowNets;
        if (flowChanged) needsRedraw = true;
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
