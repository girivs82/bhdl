// BHDL Schematic Viewer — Canvas-based circuit visualization with ELK layout
// Conventions: signal flow left-to-right, power north, ground south
(function () {
    let vscodeApi = null;
    try { vscodeApi = acquireVsCodeApi(); } catch (e) { /* standalone */ }

    const canvas = /** @type {HTMLCanvasElement} */ (document.getElementById('schematic-canvas'));
    const ctx = canvas.getContext('2d');
    const entityNameEl = document.getElementById('entity-name');
    const statsEl = document.getElementById('stats');

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
    const COLUMN_GAP = 220;
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

    function computeBoxSize(name, entityType, inPorts, outPorts, parameters) {
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

    async function computeLayout() {
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
                    if (!inst || inst.category !== 'regulator') return false;
                    const up = s.port.toUpperCase();
                    return up === 'VO' || up === 'VOUT' || up === 'OUT' || up === 'OUTPUT';
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

        // ── 6. Classify instances by placement role ──
        const instMap = new Map();
        for (const inst of data.instances) {
            if (!pgInstNames.has(inst.name)) instMap.set(inst.name, inst);
        }

        const mainPathNames = new Set();
        let shuntNames = [];        // {name, junctionName, junctionSide}
        const decouplingNames = [];  // {name, junctionName, junctionSide}
        let branchNames = [];        // {name}

        for (const [name, inst] of instMap) {
            const role = inst.placement_role;
            if (role === 'shunt' || (!role && shuntInstNames.has(name))) {
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
                        // Check if this branch component drives a signal to other components
                        if (net.driver.name === bName && net.sinks.length > 0) {
                            // Has at least one non-GND sink
                            hasSignalOutput = true;
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

        // ── 7b. Reclassify: shunts connected only to off-path → branch tail ──
        // e.g., LED connected to sense (branch) should join the branch chain
        for (let i = shuntNames.length - 1; i >= 0; i--) {
            const item = shuntNames[i];
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

        // Off-path names (computed after reclassification)
        const offPathNames = new Set([
            ...shuntNames.map(s => s.name),
            ...decouplingNames.map(d => d.name),
            ...branchNames.map(b => b.name)
        ]);

        // ── 8. Build ELK graph for main-path layout ──
        // ELK handles main-path positioning (layer assignment, spacing).
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

        function pId(nodeId, portName) { return `${nodeId}::${portName}`; }

        const allNodePorts = new Map();
        function ensurePort(nodeId, portName, side) {
            if (!allNodePorts.has(nodeId)) allNodePorts.set(nodeId, new Map());
            const pm = allNodePorts.get(nodeId);
            if (!pm.has(portName)) pm.set(portName, side);
            return pId(nodeId, portName);
        }

        const inputPorts = data.ports.filter(p => p.direction === 'in');
        const outputPorts = data.ports.filter(p => p.direction === 'out');

        // Collect ports only for main-path / power / entity nodes
        for (const p of inputPorts) ensurePort('__entity_in__', p.name, 'EAST');
        for (const p of outputPorts) ensurePort('__entity_out__', p.name, 'WEST');
        for (const ps of powerSourceNodes) ensurePort(ps.id, 'out', 'EAST');

        for (const net of processedNets) {
            const dName = net.driver.name;
            const dIsMainLike = net.driver.type === 'power_source' || net.driver.type === 'entity_port' || mainPathNames.has(dName);
            if (dIsMainLike && net.driver.type !== 'power_source' && net.driver.type !== 'entity_port') {
                ensurePort(dName, net.driver.port, 'EAST');
            }
            for (const s of net.sinks) {
                if (s.type === 'entity_port') {
                    ensurePort('__entity_out__', s.port, 'WEST');
                } else if (mainPathNames.has(s.name)) {
                    ensurePort(s.name, s.port, 'WEST');
                }
            }
        }

        function makeElkNode(id, w, h) {
            const ports = [];
            const pm = allNodePorts.get(id);
            if (pm) {
                for (const [portName, side] of pm) {
                    ports.push({
                        id: pId(id, portName),
                        width: 5, height: 5,
                        layoutOptions: { 'org.eclipse.elk.port.side': side }
                    });
                }
            }
            return { id, width: w, height: h, ports };
        }

        // ELK children: only main-path components (no shunts, no branches)
        const elkChildren = [];
        const elkEdges = [];
        let edgeIdx = 0;

        for (const ps of powerSourceNodes) {
            const w = Math.max(80, measureTextWidth(ps.label, FONT_SIZE) + 24);
            elkChildren.push(makeElkNode(ps.id, w, 28));
        }

        if (inputPorts.length > 0) {
            const h = HEADER_HEIGHT + ENTITY_PADDING * 2 + inputPorts.length * PORT_SPACING;
            let maxW = 0;
            for (const p of inputPorts) maxW = Math.max(maxW, measureTextWidth(p.name, FONT_SIZE));
            elkChildren.push(makeElkNode('__entity_in__', Math.max(ENTITY_BOX_MIN_WIDTH, maxW + 50), h));
        }

        for (const name of mainPathOrder) {
            const inst = instMap.get(name);
            if (!inst) continue;
            const { inP, outP } = getInstPorts(inst);
            const size = computeBoxSize(name, inst.entity_type, inP, outP, inst.parameters);
            elkChildren.push(makeElkNode(name, size.w, size.h));
        }

        if (outputPorts.length > 0) {
            const h = HEADER_HEIGHT + ENTITY_PADDING * 2 + outputPorts.length * PORT_SPACING;
            let maxW = 0;
            for (const p of outputPorts) maxW = Math.max(maxW, measureTextWidth(p.name, FONT_SIZE));
            elkChildren.push(makeElkNode('__entity_out__', Math.max(ENTITY_BOX_MIN_WIDTH, maxW + 50), h));
        }

        // ELK edges: only main-path ↔ main-path connections
        for (const net of processedNets) {
            const driverElName = net.driver.type === 'power_source' ? net.driver.name
                : net.driver.type === 'entity_port' ? '__entity_in__'
                : net.driver.name;
            const dIsMainLike = mainPathNames.has(driverElName) || net.driver.type === 'power_source' || driverElName === '__entity_in__';
            if (!dIsMainLike) continue;

            for (const sink of net.sinks) {
                const sinkElName = sink.type === 'entity_port' ? '__entity_out__' : sink.name;
                if (sinkElName !== '__entity_out__' && !mainPathNames.has(sinkElName)) continue;

                elkEdges.push({
                    id: `e${edgeIdx++}`,
                    sources: [pId(driverElName, net.driver.port)],
                    targets: [pId(sinkElName, sink.port)],
                    layoutOptions: { 'org.eclipse.elk.priority': '10' }
                });
            }
        }

        const elkGraph = {
            id: 'root',
            layoutOptions: {
                'algorithm': 'layered',
                'org.eclipse.elk.direction': 'RIGHT',
                'org.eclipse.elk.portConstraints': 'FIXED_SIDE',
                'org.eclipse.elk.spacing.nodeNode': '50',
                'org.eclipse.elk.layered.spacing.nodeNodeBetweenLayers': '100',
                'org.eclipse.elk.edgeRouting': 'ORTHOGONAL',
                'org.eclipse.elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
                'org.eclipse.elk.layered.compaction.postCompaction.strategy': 'EDGE_LENGTH',
            },
            children: elkChildren,
            edges: elkEdges
        };

        const elk = new ELK();
        const elkResult = await elk.layout(elkGraph);

        // ── 9. Extract main-path positions from ELK ──
        const positions = new Map();
        for (const child of elkResult.children) {
            positions.set(child.id, { x: child.x, y: child.y, w: child.width, h: child.height });
        }

        // ── 9a. Align main-path nodes by first-port Y position ──
        // ELK can produce slight Y offsets between layers. We force all
        // main-band nodes to have their first port at the same Y so wires
        // are perfectly horizontal through the main path.
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
                if (pos) maxPortY = Math.max(maxPortY, pos.y + instPortOffset);
            }
            // Reposition each node so its first port is at maxPortY
            for (const ps of powerSourceNodes) {
                const pos = positions.get(ps.id);
                if (pos) pos.y = maxPortY - pos.h / 2;
            }
            for (const name of mainPathOrder) {
                const pos = positions.get(name);
                if (pos) pos.y = maxPortY - instPortOffset;
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
            const j = findJunction(item.name);
            if (j) {
                item.junctionName = j.name;
                item.junctionSide = j.side;
                item.junctionNet = j.netName;
            }
        }

        // ── 9b. Gap expansion: ensure main-path gaps are wide enough for off-path items ──
        // Pre-compute sizes of off-path items
        const offPathSizes = new Map();
        for (const item of [...shuntNames, ...decouplingNames, ...branchNames]) {
            const inst = instMap.get(item.name);
            if (!inst) continue;
            const { inP, outP } = getInstPorts(inst);
            offPathSizes.set(item.name, computeBoxSize(item.name, inst.entity_type, inP, outP, inst.parameters));
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
        const MIN_ITEM_GAP = 30;  // minimum gap between off-path items
        const MIN_EDGE_PAD = 30;  // minimum padding from main-path node edges

        // Compute total width needed for a group of off-path items
        function groupTotalWidth(items) {
            let total = 0;
            for (const item of items) {
                const sz = offPathSizes.get(item.name);
                if (sz) total += sz.w;
            }
            // Add gaps between items
            if (items.length > 1) total += (items.length - 1) * MIN_ITEM_GAP;
            return total;
        }

        // For each pair of consecutive main-path nodes, compute space needed
        // We'll iterate through mainPathOrder plus power sources (which are left of everything)
        // Build ordered list of all main-band nodes as they appear left-to-right
        const mainBandOrder = [];
        // Power sources come first (already positioned by ELK)
        for (const ps of powerSourceNodes) {
            mainBandOrder.push(ps.id);
        }
        for (const name of mainPathOrder) {
            mainBandOrder.push(name);
        }

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
        const shuntY = mainBandBottom + SHUNT_DROP;

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
                    rx -= MIN_ITEM_GAP;
                }
            } else {
                // Right side: leftmost item offset right of output port dot, grow right
                const portDotX = jPos.x + jPos.w + PORT_STUB_LEN + SHUNT_PORT_OFFSET;
                let lx = portDotX - itemSizes[0].w / 2;
                for (let i = 0; i < items.length; i++) {
                    const sz = itemSizes[i];
                    positions.set(items[i].name, { x: lx, y: shuntY, w: sz.w, h: sz.h });
                    lx += sz.w + MIN_ITEM_GAP;
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
                const minX = prev.x + prev.w + MIN_ITEM_GAP;
                if (curr.x < minX) curr.x = minX;
            }
        }

        // ── 10c. Place branches as horizontal chains ──
        for (const [, group] of branchGroups) {
            const jPos = positions.get(group.junctionName);
            if (!jPos) continue;
            const ordered = orderBranchChain(group.items, processedNets);

            let bx;
            if (group.side === 'left') {
                bx = jPos.x - 200;
            } else {
                bx = jPos.x + jPos.w + 20;
                for (const dItem of verticalDropItems) {
                    if (dItem.junctionName === group.junctionName && dItem.junctionSide === group.side) {
                        const dPos = positions.get(dItem.name);
                        if (dPos) bx = Math.max(bx, dPos.x + dPos.w + 30);
                    }
                }
            }

            for (const item of ordered) {
                const sz = offPathSizes.get(item.name) || { w: INSTANCE_BOX_MIN_WIDTH, h: 60 };
                positions.set(item.name, { x: bx, y: shuntY, w: sz.w, h: sz.h });
                bx += sz.w + 120;
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
            const isShuntLike = offPathNames.has(name) && !branchNames.some(b => b.name === name);

            const instInPorts = [], instOutPorts = [];
            const seenIn = new Set(), seenOut = new Set();
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
                } else {
                    if (r.has('in') && !seenIn.has(c.port)) {
                        seenIn.add(c.port);
                        const py = pos.y + HEADER_HEIGHT + INSTANCE_PADDING + (instInPorts.length + 0.5) * PORT_SPACING;
                        instInPorts.push({ name: c.port, x: pos.x, y: py, pinType: getPortPinType(name, c.port), isClock: clockSignals.has(c.signal), isReset: resetSignals.has(c.signal) });
                    }
                    if (r.has('out') && !seenOut.has(c.port)) {
                        seenOut.add(c.port);
                        const py = pos.y + HEADER_HEIGHT + INSTANCE_PADDING + (instOutPorts.length + 0.5) * PORT_SPACING;
                        instOutPorts.push({ name: c.port, x: pos.x + pos.w, y: py, pinType: getPortPinType(name, c.port) });
                    }
                }
            }
            // Attach simulation annotations (current, power) if available
            const simCurrent = data.simulation?.instance_currents?.[name];
            const simPowerW = data.simulation?.instance_power?.[name];
            layoutElements.push({ x: pos.x, y: pos.y, w: pos.w, h: pos.h, name, type: 'instance', entityType: inst.entity_type, parameters: inst.parameters, category: inst.category, isShunt: isShuntLike, inputPorts: instInPorts, outputPorts: instOutPorts, gndStubs: gndStubsByInst.get(name) || [], pgStubs: [], line: inst.line, simCurrent, simPower: simPowerW });
        }

        // Entity output
        if (outputPorts.length > 0 && positions.has('__entity_out__')) {
            const pos = positions.get('__entity_out__');
            const inP = outputPorts.map((p, i) => ({
                name: p.name, x: pos.x, y: pos.y + HEADER_HEIGHT + ENTITY_PADDING + (i + 0.5) * PORT_SPACING
            }));
            layoutElements.push({ x: pos.x, y: pos.y, w: pos.w, h: pos.h, name: data.entity_name, type: 'entity_out', inputPorts: inP, outputPorts: [], gndStubs: [], pgStubs: [], line: data.entity_line });
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
                const dx = sides[li] === 'out' ? PORT_STUB_LEN : -PORT_STUB_LEN;
                return { x: p.x + dx, y: p.y };
            }
            return null;
        }

        const junctionPoints = [];

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

                if (isShuntWire || toPos.y > fromPos.y + 20) {
                    if (toPos.x >= fromPos.x - 2) {
                        // Normal L-route: horizontal forward to shunt X, then vertical drop
                        const jx = toPos.x;
                        const jy = fromPos.y;
                        junctionPoints.push({ x: jx, y: jy });
                        if (Math.abs(fromPos.x - jx) > 2) {
                            segments.push({ x1: fromPos.x, y1: fromPos.y, x2: jx, y2: jy });
                        }
                        segments.push({ x1: jx, y1: jy, x2: toPos.x, y2: toPos.y });
                    } else {
                        // Reverse L-route: shunt is behind driver port.
                        // Drop straight down from driver, then horizontal back to shunt.
                        junctionPoints.push({ x: fromPos.x, y: fromPos.y });
                        segments.push({ x1: fromPos.x, y1: fromPos.y, x2: fromPos.x, y2: toPos.y });
                        segments.push({ x1: fromPos.x, y1: toPos.y, x2: toPos.x, y2: toPos.y });
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
                    // Simulation available: a net is power if GLACIER says so
                    isPowerNet = simPower instanceof Set ? simPower.has(net.name) : !!simPower[net.name];
                } else {
                    // Fallback: driver pin_type heuristic
                    const driverPinType = net.driver.type === 'power_source' ? 'power'
                        : getPortPinType(net.driver.name, net.driver.port);
                    isPowerNet = driverPinType === 'power' || net.net_class === 'power';
                }

                // Gather simulation annotations for this wire
                const voltage = data.simulation?.net_voltages?.[net.name];
                // Current: use the sink component's current (current flowing into it through this wire)
                // GLACIER decomposes some components (e.g. regulators → name_dropout + name_vout),
                // so fall back to the sub-component with the largest absolute current.
                let current = data.simulation?.instance_currents?.[sink.name];
                if (current == null && data.simulation?.instance_currents) {
                    let maxAbs = 0;
                    for (const [key, val] of Object.entries(data.simulation.instance_currents)) {
                        if (key.startsWith(sink.name + '_') && Math.abs(val) > maxAbs) {
                            maxAbs = Math.abs(val);
                            current = val;
                        }
                    }
                }
                const driverIsPowerSource = net.driver.type === 'power_source';
                layoutWires.push({ from: fromPos, to: toPos, segments, width: net.width || 1, netName: net.name, netClass: net.net_class || 'signal', isPower: isPowerNet, voltage, current, driverIsPowerSource });
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

        drawWires();
        for (const el of layoutElements) {
            if (el.type === 'entity_in' || el.type === 'entity_out') drawEntityBox(el);
            else if (el.type === 'power_source') drawPowerSourceNode(el);
            else drawInstanceBox(el);
        }
        // Draw GND stubs on top
        for (const el of layoutElements) {
            if (el.gndStubs && el.gndStubs.length > 0) drawGndStubs(el);
        }

        ctx.restore();

        // Draw simulation tooltip for hovered items (in screen space)
        drawSimTooltip();
    }

    function drawSimTooltip() {
        if (!schematicData?.simulation) return;
        const sim = schematicData.simulation;
        const lines = [];

        if (hoveredItem) {
            const el = layoutElements.find(e => e.name === hoveredItem);
            if (el && el.type === 'instance') {
                lines.push(el.name + ' (' + (el.entityType || '') + ')');
                if (el.simCurrent != null) lines.push('I = ' + formatCurrent(el.simCurrent));
                if (el.simPower != null) lines.push('P = ' + formatPower(el.simPower));
            }
        } else if (hoveredNet) {
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

        // Instance name
        ctx.fillStyle = COLORS.text;
        ctx.font = `bold ${FONT_SIZE}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(el.name, el.x + el.w / 2, el.y + HEADER_HEIGHT / 2);

        // Entity type
        if (el.entityType) {
            ctx.fillStyle = COLORS.textDim;
            ctx.font = `${FONT_SIZE - 1}px monospace`;
            ctx.fillText(el.entityType, el.x + el.w / 2, el.y + HEADER_HEIGHT + 10);
        }

        // Parameters
        if (el.parameters && el.parameters.length > 0) {
            ctx.fillStyle = COLORS.paramText;
            ctx.font = `${FONT_SIZE - 2}px monospace`;
            const paramStr = el.parameters.filter(p => p[1]).map(p => p[1]).join(', ');
            if (paramStr) ctx.fillText(paramStr, el.x + el.w / 2, el.y + HEADER_HEIGHT + 22);
        }

        const showLabels = shouldShowPortLabels(el.category);

        if (el.isShunt) {
            // Shunt component: single port on NORTH (top), GND stub on SOUTH (bottom)
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
            return; // Skip standard left/right port rendering
        }

        function portColor(port) {
            if (port.pinType === 'power') return COLORS.portPower;
            if (port.isClock || port.pinType === 'clock') return COLORS.portClock;
            if (port.isReset || port.pinType === 'reset') return COLORS.portReset;
            return COLORS.port;
        }

        // Input ports (left)
        for (const port of el.inputPorts) {
            const pc = portColor(port);
            ctx.strokeStyle = pc;
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.moveTo(port.x - PORT_STUB_LEN, port.y);
            ctx.lineTo(port.x, port.y);
            ctx.stroke();
            ctx.fillStyle = pc;
            ctx.beginPath();
            ctx.arc(port.x - PORT_STUB_LEN, port.y, PORT_DOT_R - 0.5, 0, Math.PI * 2);
            ctx.fill();
            if (showLabels) {
                ctx.fillStyle = COLORS.text;
                ctx.font = `${FONT_SIZE - 1}px monospace`;
                ctx.textAlign = 'left';
                ctx.textBaseline = 'middle';
                ctx.fillText(port.name, port.x + 4, port.y);
            }
        }

        // Output ports (right)
        for (const port of el.outputPorts) {
            const pc = portColor(port);
            ctx.strokeStyle = pc;
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.moveTo(port.x, port.y);
            ctx.lineTo(port.x + PORT_STUB_LEN, port.y);
            ctx.stroke();
            ctx.fillStyle = pc;
            ctx.beginPath();
            ctx.arc(port.x + PORT_STUB_LEN, port.y, PORT_DOT_R - 0.5, 0, Math.PI * 2);
            ctx.fill();
            if (showLabels) {
                ctx.fillStyle = COLORS.text;
                ctx.font = `${FONT_SIZE - 1}px monospace`;
                ctx.textAlign = 'right';
                ctx.textBaseline = 'middle';
                ctx.fillText(port.name, port.x - 4, port.y);
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

            // Vertical line down
            ctx.strokeStyle = COLORS.groundStub;
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.moveTo(cx, botY);
            ctx.lineTo(cx, botY + GND_STUB_HEIGHT);
            ctx.stroke();

            // Ground symbol: 3 narrowing horizontal lines
            for (let g = 0; g < GND_LINE_WIDTHS.length; g++) {
                const lw = GND_LINE_WIDTHS[g];
                const ly = botY + GND_STUB_HEIGHT + g * GND_LINE_SPACING;
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

    function drawWires() {
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

            // Net annotations on horizontal wire segments
            // Skip net name if driver is a power source node — the box already shows it
            if (wire.segments.length > 0) {
                const seg = wire.segments[0];
                const isHoriz = Math.abs(seg.x2 - seg.x1) > 60;
                if (isHoriz && !wire.driverIsPowerSource) {
                    const lx = (seg.x1 + seg.x2) / 2;
                    ctx.font = `${FONT_SIZE - 2}px monospace`;
                    ctx.textAlign = 'center';
                    ctx.textBaseline = 'bottom';
                    if (wire.voltage != null && isPower) {
                        ctx.fillStyle = COLORS.portPower;
                        ctx.fillText(`${wire.netName} (${formatVoltage(wire.voltage)})`, lx, seg.y1 - 6);
                    } else if (wire.voltage != null && isHighlighted) {
                        ctx.fillStyle = COLORS.wireHighlight;
                        ctx.fillText(`${wire.netName} (${formatVoltage(wire.voltage)})`, lx, seg.y1 - 6);
                    } else {
                        ctx.fillStyle = COLORS.textMuted;
                        ctx.fillText(wire.netName, lx, seg.y1 - 6);
                    }
                }
                // Current annotation on the wire
                if (wire.current != null && Math.abs(wire.current) > 1e-9) {
                    const hSeg = wire.segments.find(s => Math.abs(s.x2 - s.x1) > 40);
                    if (hSeg) {
                        // Horizontal segment: label below
                        const cx = (hSeg.x1 + hSeg.x2) / 2;
                        ctx.font = `${FONT_SIZE - 2}px monospace`;
                        ctx.textAlign = 'center';
                        ctx.textBaseline = 'top';
                        ctx.fillStyle = '#8bc34a';
                        ctx.fillText(formatCurrent(Math.abs(wire.current)), cx, hSeg.y1 + 4);
                    } else {
                        // No horizontal segment — try vertical (e.g. shunt/drop wires)
                        const vSeg = wire.segments.find(s => Math.abs(s.y2 - s.y1) > 20);
                        if (vSeg) {
                            const cy = (vSeg.y1 + vSeg.y2) / 2;
                            ctx.font = `${FONT_SIZE - 2}px monospace`;
                            ctx.textAlign = 'left';
                            ctx.textBaseline = 'middle';
                            ctx.fillStyle = '#8bc34a';
                            ctx.fillText(formatCurrent(Math.abs(wire.current)), vSeg.x1 + 6, cy);
                        }
                    }
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
        computeLayout().then(() => zoomToFit());
    }

    window.addEventListener('message', (event) => {
        if (event.data.type === 'updateSchematic') loadSchematicData(event.data.data);
    });
    if (window.__BHDL_SCHEMATIC_DATA__) loadSchematicData(window.__BHDL_SCHEMATIC_DATA__);
    window.addEventListener('resize', resizeCanvas);
    resizeCanvas();
})();
