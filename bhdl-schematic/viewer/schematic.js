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
        portIn: '#64b5f6', portOut: '#ff9800', portInout: '#9c27b0',
        portClock: '#4caf50', portReset: '#ef5350',
        portPower: '#ff6b6b', portGround: '#888888', portPassive: '#ffa726',
        wire: '#5c8dbf', wireBus: '#7baad4',
        wireHighlight: '#ffeb3b',
        text: '#d4d4d4', textDim: '#777', textMuted: '#555',
        paramText: '#9e9e9e',
        busSlash: '#8cb4d8', busLabel: '#8cb4d8',
        highlight: '#ffeb3b', junctionDot: '#8cb4d8',
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
        const inputPorts = data.ports.filter(p => p.direction === 'in');
        const outputPorts = data.ports.filter(p => p.direction === 'out');

        // ── Step 1: Identify PG symbols and GND nets ──
        const pgInstNames = new Set();
        const gndNetNames = new Set();

        for (const inst of data.instances) {
            if (isPowerGroundSymbol(inst)) pgInstNames.add(inst.name);
        }
        for (const net of (data.nets || [])) {
            if (net.net_class === 'ground') gndNetNames.add(net.name);
        }

        // ── Step 2: Process nets — remove PG symbol refs, reassign drivers ──
        const powerSourceNodes = []; // synthetic VIN-like source nodes
        let processedNets = [];

        for (const net of (data.nets || [])) {
            if (net.net_class === 'ground') continue; // GND → stubs only

            let driver = { ...net.driver };
            let sinks = net.sinks.map(s => ({ ...s }));
            const driverIsPgSymbol = pgInstNames.has(driver.name);
            sinks = sinks.filter(s => !pgInstNames.has(s.name));

            if (driverIsPgSymbol) {
                // Try to find regulator output as new driver
                const regIdx = sinks.findIndex(s => {
                    const inst = data.instances.find(i => i.name === s.name);
                    if (!inst || inst.category !== 'regulator') return false;
                    const up = s.port.toUpperCase();
                    return up === 'VO' || up === 'VOUT' || up === 'OUT' || up === 'OUTPUT';
                });

                if (regIdx >= 0) {
                    driver = sinks.splice(regIdx, 1)[0];
                } else if (sinks.length > 0) {
                    // Create synthetic power source node
                    const label = net.voltage != null ? `${net.name} (${net.voltage}V)` : net.name;
                    const sourceId = `__pwr_${net.name}__`;
                    powerSourceNodes.push({ id: sourceId, label, voltage: net.voltage });
                    driver = { type: 'power_source', name: sourceId, port: 'out' };
                } else {
                    continue;
                }
            }

            if (sinks.length === 0) continue;

            processedNets.push({
                name: net.name, width: net.width || 1,
                net_class: net.net_class, voltage: net.voltage,
                driver, sinks
            });
        }

        // ── Step 3: Identify shunt components and merge pass-through nets ──
        // A shunt component has 1 signal pin + GND pin(s). Its signal pin
        // may appear as both a net sink and a net driver (pass-through junction).
        // Merging these nets reveals the correct topology where shunt components
        // branch off the main wire rather than being inline.

        const shuntPinKeys = new Set();
        for (const inst of data.instances) {
            if (pgInstNames.has(inst.name)) continue;
            const signalPorts = new Set();
            const gndPorts = new Set();
            for (const conn of inst.connections) {
                if (gndNetNames.has(conn.signal)) gndPorts.add(conn.port);
                else signalPorts.add(conn.port);
            }
            // 2-pin shunt: exactly 1 unique signal port + at least 1 GND port
            if (signalPorts.size === 1 && gndPorts.size >= 1) {
                const portName = signalPorts.values().next().value;
                shuntPinKeys.add(`${inst.name}.${portName}`);
            }
        }

        // Set of instance names that are shunt components (vertical top-bottom orientation)
        const shuntInstNames = new Set();
        for (const key of shuntPinKeys) shuntInstNames.add(key.split('.')[0]);

        // Iteratively merge: if shunt pin X is sink of net A and driver of net B,
        // merge A ← B (A absorbs B's sinks, X stays as branch sink of A)
        let mergeChanged = true;
        while (mergeChanged) {
            mergeChanged = false;
            for (const pinKey of shuntPinKeys) {
                const downIdx = processedNets.findIndex(n =>
                    n && n.driver.type !== 'power_source' &&
                    `${n.driver.name}.${n.driver.port}` === pinKey);
                if (downIdx < 0) continue;

                const upIdx = processedNets.findIndex(n =>
                    n && n.sinks.some(s => `${s.name}.${s.port}` === pinKey));
                if (upIdx < 0 || upIdx === downIdx) continue;

                // Merge: upstream net absorbs downstream net's sinks
                processedNets[upIdx].sinks = [
                    ...processedNets[upIdx].sinks,
                    ...processedNets[downIdx].sinks
                ];
                processedNets.splice(downIdx, 1);
                mergeChanged = true;
                break; // restart after modification
            }
        }

        // ── Step 4: Collect GND stubs per instance (from ground nets) ──
        const gndStubsByInst = new Map();
        for (const net of (data.nets || [])) {
            if (net.net_class !== 'ground') continue;
            const allEps = [net.driver, ...net.sinks];
            for (const ep of allEps) {
                if (!ep || ep.type === 'entity_port' || pgInstNames.has(ep.name)) continue;
                if (!gndStubsByInst.has(ep.name)) gndStubsByInst.set(ep.name, []);
                const arr = gndStubsByInst.get(ep.name);
                if (!arr.some(s => s.port === ep.port)) {
                    arr.push({ port: ep.port });
                }
            }
        }

        // ── Step 5: Build port info from processed nets ──
        // For each instance port, determine if it's an input (WEST) or output (EAST)
        const instPortRoles = new Map(); // "inst.port" -> Set('in'|'out')
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

        // ── Step 6: Build ELK graph ──
        const elkNodes = [];
        const elkEdges = [];
        const netWidthMap = new Map();
        const netNameMap = new Map();
        const netClassEdgeMap = new Map();

        // Entity input port bar
        if (inputPorts.length > 0) {
            const h = HEADER_HEIGHT + ENTITY_PADDING * 2 + inputPorts.length * PORT_SPACING;
            let maxW = 0;
            for (const p of inputPorts) maxW = Math.max(maxW, measureTextWidth(p.name, FONT_SIZE));
            elkNodes.push({
                id: '__entity_in__',
                width: Math.max(ENTITY_BOX_MIN_WIDTH, maxW + 50), height: h,
                layoutOptions: { 'elk.layered.layerConstraint': 'FIRST', 'elk.portConstraints': 'FIXED_ORDER' },
                ports: inputPorts.map((p, i) => ({
                    id: '__entity_in___' + p.name + '_out', width: 1, height: 1,
                    layoutOptions: { 'elk.port.side': 'EAST', 'elk.port.index': String(i) }
                }))
            });
        }

        // Synthetic power source nodes (e.g., VIN)
        for (const ps of powerSourceNodes) {
            const w = Math.max(80, measureTextWidth(ps.label, FONT_SIZE) + 24);
            elkNodes.push({
                id: ps.id, width: w, height: 28,
                layoutOptions: { 'elk.layered.layerConstraint': 'FIRST', 'elk.portConstraints': 'FIXED_ORDER' },
                ports: [{
                    id: ps.id + '_out', width: 1, height: 1,
                    layoutOptions: { 'elk.port.side': 'EAST', 'elk.port.index': '0' }
                }]
            });
        }

        // Instance nodes (skip PG symbols)
        for (const inst of data.instances) {
            if (pgInstNames.has(inst.name)) continue;

            // Build port list from net roles (not from instance connection data)
            const portList = [];
            const seenPortDir = new Set(); // "port_dir" dedup
            const roles = new Map();

            // Collect roles for this instance's ports
            for (const conn of inst.connections) {
                if (gndNetNames.has(conn.signal)) continue; // GND → stubs, not ports
                const key = `${inst.name}.${conn.port}`;
                const r = instPortRoles.get(key);
                if (!r) continue;
                roles.set(conn.port, r);
            }

            const inPorts = [];
            const outPorts = [];
            for (const [portName, dirs] of roles) {
                if (dirs.has('in') && !seenPortDir.has(portName + '_in')) {
                    seenPortDir.add(portName + '_in');
                    inPorts.push(portName);
                }
                if (dirs.has('out') && !seenPortDir.has(portName + '_out')) {
                    seenPortDir.add(portName + '_out');
                    outPorts.push(portName);
                }
            }

            const isShunt = shuntInstNames.has(inst.name) && inPorts.length >= 1 && outPorts.length === 0;
            const size = computeBoxSize(inst.name, inst.entity_type, inPorts, outPorts, inst.parameters);
            const ports = [];

            if (isShunt) {
                // Shunt component: signal port on NORTH (top-bottom orientation)
                ports.push({
                    id: inst.name + '_' + inPorts[0] + '_in', width: 1, height: 1,
                    layoutOptions: { 'elk.port.side': 'NORTH', 'elk.port.index': '0' }
                });
            } else {
                for (let j = 0; j < inPorts.length; j++) {
                    ports.push({
                        id: inst.name + '_' + inPorts[j] + '_in', width: 1, height: 1,
                        layoutOptions: { 'elk.port.side': 'WEST', 'elk.port.index': String(j) }
                    });
                }
                for (let j = 0; j < outPorts.length; j++) {
                    ports.push({
                        id: inst.name + '_' + outPorts[j] + '_out', width: 1, height: 1,
                        layoutOptions: { 'elk.port.side': 'EAST', 'elk.port.index': String(j) }
                    });
                }
            }

            elkNodes.push({
                id: inst.name, width: size.w, height: size.h, ports,
                layoutOptions: { 'elk.portConstraints': 'FIXED_ORDER' }
            });
        }

        // Entity output port bar
        if (outputPorts.length > 0) {
            const h = HEADER_HEIGHT + ENTITY_PADDING * 2 + outputPorts.length * PORT_SPACING;
            let maxW = 0;
            for (const p of outputPorts) maxW = Math.max(maxW, measureTextWidth(p.name, FONT_SIZE));
            elkNodes.push({
                id: '__entity_out__',
                width: Math.max(ENTITY_BOX_MIN_WIDTH, maxW + 50), height: h,
                layoutOptions: { 'elk.layered.layerConstraint': 'LAST', 'elk.portConstraints': 'FIXED_ORDER' },
                ports: outputPorts.map((p, i) => ({
                    id: '__entity_out___' + p.name + '_in', width: 1, height: 1,
                    layoutOptions: { 'elk.port.side': 'WEST', 'elk.port.index': String(i) }
                }))
            });
        }

        // Valid port IDs for edge validation
        const validPortIds = new Set();
        for (const node of elkNodes) {
            for (const port of (node.ports || [])) validPortIds.add(port.id);
        }

        // Nets → ELK edges
        let edgeIdx = 0;
        for (const net of processedNets) {
            const driverPortId = net.driver.type === 'power_source'
                ? net.driver.name + '_out'
                : net.driver.type === 'entity_port'
                    ? '__entity_in___' + net.driver.port + '_out'
                    : net.driver.name + '_' + net.driver.port + '_out';

            if (!validPortIds.has(driverPortId)) continue;

            for (const sink of net.sinks) {
                const sinkPortId = sink.type === 'entity_port'
                    ? '__entity_out___' + sink.port + '_in'
                    : sink.name + '_' + sink.port + '_in';

                if (!validPortIds.has(sinkPortId)) continue;

                const edgeId = 'e' + edgeIdx++;
                netWidthMap.set(edgeId, net.width);
                netNameMap.set(edgeId, net.name);
                netClassEdgeMap.set(edgeId, net.net_class || 'signal');
                elkEdges.push({ id: edgeId, sources: [driverPortId], targets: [sinkPortId] });
            }
        }

        const graph = {
            id: 'root',
            layoutOptions: {
                'elk.algorithm': 'layered',
                'elk.direction': 'RIGHT',
                'elk.layered.spacing.nodeNodeBetweenLayers': String(COLUMN_GAP),
                'elk.spacing.nodeNode': String(ROW_GAP),
                'elk.edgeRouting': 'ORTHOGONAL',
                'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
                'elk.layered.nodePlacement.strategy': 'BRANDES_KOEPF',
                'elk.spacing.portPort': String(PORT_SPACING),
                'elk.layered.spacing.edgeEdgeBetweenLayers': '15',
                'elk.layered.spacing.edgeNodeBetweenLayers': '20'
            },
            children: elkNodes,
            edges: elkEdges
        };

        const elk = new ELK();
        let result;
        try {
            result = await elk.layout(graph);
        } catch (err) {
            console.error('ELK layout failed:', err);
            return;
        }

        // ── Extract results ──
        for (const child of (result.children || [])) {
            const portMap = new Map();
            for (const port of (child.ports || [])) {
                portMap.set(port.id, { x: child.x + port.x, y: child.y + port.y });
            }

            // Synthetic power source node
            const psNode = powerSourceNodes.find(ps => ps.id === child.id);
            if (psNode) {
                const outPort = portMap.get(psNode.id + '_out');
                layoutElements.push({
                    x: child.x, y: child.y, w: child.width, h: child.height,
                    name: psNode.id, type: 'power_source', label: psNode.label,
                    inputPorts: [],
                    outputPorts: outPort ? [{ name: 'out', x: outPort.x, y: outPort.y }] : [],
                    gndStubs: [], pgStubs: []
                });
                continue;
            }

            if (child.id === '__entity_in__') {
                const outP = [];
                for (const p of inputPorts) {
                    const pos = portMap.get('__entity_in___' + p.name + '_out');
                    if (pos) outP.push({ name: p.name, x: pos.x, y: pos.y, isClock: p.type === 'clock', isReset: p.type === 'reset' });
                }
                layoutElements.push({
                    x: child.x, y: child.y, w: child.width, h: child.height,
                    name: data.entity_name, type: 'entity_in',
                    inputPorts: [], outputPorts: outP, gndStubs: [], pgStubs: [],
                    line: data.entity_line
                });
            } else if (child.id === '__entity_out__') {
                const inP = [];
                for (const p of outputPorts) {
                    const pos = portMap.get('__entity_out___' + p.name + '_in');
                    if (pos) inP.push({ name: p.name, x: pos.x, y: pos.y });
                }
                layoutElements.push({
                    x: child.x, y: child.y, w: child.width, h: child.height,
                    name: data.entity_name, type: 'entity_out',
                    inputPorts: inP, outputPorts: [], gndStubs: [], pgStubs: [],
                    line: data.entity_line
                });
            } else {
                const inst = data.instances.find(i => i.name === child.id);
                if (!inst) continue;
                const roles = instPortRoles;

                const instInPorts = [];
                const instOutPorts = [];

                // Build ports from net-derived roles
                const seenIn = new Set(), seenOut = new Set();
                for (const conn of inst.connections) {
                    if (gndNetNames.has(conn.signal)) continue;
                    const key = `${inst.name}.${conn.port}`;
                    const r = roles.get(key);
                    if (!r) continue;

                    if (r.has('in') && !seenIn.has(conn.port)) {
                        seenIn.add(conn.port);
                        const pos = portMap.get(inst.name + '_' + conn.port + '_in');
                        if (pos) {
                            instInPorts.push({
                                name: conn.port, x: pos.x, y: pos.y,
                                pinType: getPortPinType(inst.name, conn.port),
                                isClock: clockSignals.has(conn.signal),
                                isReset: resetSignals.has(conn.signal)
                            });
                        }
                    }
                    if (r.has('out') && !seenOut.has(conn.port)) {
                        seenOut.add(conn.port);
                        const pos = portMap.get(inst.name + '_' + conn.port + '_out');
                        if (pos) {
                            instOutPorts.push({
                                name: conn.port, x: pos.x, y: pos.y,
                                pinType: getPortPinType(inst.name, conn.port)
                            });
                        }
                    }
                }

                const isShuntEl = shuntInstNames.has(inst.name) && instInPorts.length >= 1 && instOutPorts.length === 0;
                layoutElements.push({
                    x: child.x, y: child.y, w: child.width, h: child.height,
                    name: inst.name, type: 'instance',
                    entityType: inst.entity_type,
                    parameters: inst.parameters,
                    category: inst.category,
                    isShunt: isShuntEl,
                    inputPorts: instInPorts, outputPorts: instOutPorts,
                    gndStubs: gndStubsByInst.get(inst.name) || [],
                    pgStubs: [],
                    line: inst.line
                });
            }
        }

        // Extract wire routing from ELK edges
        for (const edge of (result.edges || [])) {
            const segments = [];
            let fromPt = null, toPt = null;
            for (const section of (edge.sections || [])) {
                const pts = [section.startPoint];
                if (section.bendPoints) for (const bp of section.bendPoints) pts.push(bp);
                pts.push(section.endPoint);
                if (!fromPt) fromPt = { x: pts[0].x, y: pts[0].y };
                toPt = { x: pts[pts.length - 1].x, y: pts[pts.length - 1].y };
                for (let i = 0; i < pts.length - 1; i++) {
                    segments.push({ x1: pts[i].x, y1: pts[i].y, x2: pts[i + 1].x, y2: pts[i + 1].y });
                }
            }
            if (fromPt && toPt) {
                layoutWires.push({
                    from: fromPt, to: toPt, segments,
                    width: netWidthMap.get(edge.id) || 1,
                    netName: netNameMap.get(edge.id) || '',
                    netClass: netClassEdgeMap.get(edge.id) || 'signal'
                });
            }
        }
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
    }

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
            const color = port.isClock ? COLORS.portClock : port.isReset ? COLORS.portReset : isInput ? COLORS.portIn : COLORS.portOut;
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
                ctx.strokeStyle = COLORS.portIn;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(port.x, port.y);
                ctx.lineTo(port.x, port.y - PORT_STUB_LEN);
                ctx.stroke();
                ctx.fillStyle = COLORS.portIn;
                ctx.beginPath();
                ctx.arc(port.x, port.y - PORT_STUB_LEN, PORT_DOT_R, 0, Math.PI * 2);
                ctx.fill();
            }
            return; // Skip standard left/right port rendering
        }

        // Input ports (left)
        for (const port of el.inputPorts) {
            const pc = port.pinType === 'power' ? COLORS.portPower :
                       port.pinType === 'ground' ? COLORS.portGround :
                       port.pinType === 'clock' ? COLORS.portClock :
                       port.pinType === 'reset' ? COLORS.portReset :
                       COLORS.portIn;
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
            const pc = port.pinType === 'power' ? COLORS.portPower :
                       port.pinType === 'ground' ? COLORS.portGround :
                       COLORS.portOut;
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
            const isPower = wire.netClass === 'power';
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

            // Junction dots
            ctx.fillStyle = isPower ? COLORS.portPower : COLORS.junctionDot;
            ctx.beginPath();
            ctx.arc(wire.from.x, wire.from.y, isBus ? 3 : 2, 0, Math.PI * 2);
            ctx.fill();
            ctx.beginPath();
            ctx.arc(wire.to.x, wire.to.y, isBus ? 3 : 2, 0, Math.PI * 2);
            ctx.fill();

            // Net name label
            if (wire.segments.length > 0) {
                const seg = wire.segments[0];
                if (Math.abs(seg.x2 - seg.x1) > 60) {
                    const lx = (seg.x1 + seg.x2) / 2;
                    ctx.fillStyle = COLORS.textMuted;
                    ctx.font = `${FONT_SIZE - 2}px monospace`;
                    ctx.textAlign = 'center';
                    ctx.textBaseline = 'bottom';
                    ctx.fillText(wire.netName, lx, seg.y1 - 6);
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
        statsEl.textContent = `${portCount} ports, ${instCount} components, ${netCount} nets`;
        computeLayout().then(() => zoomToFit());
    }

    window.addEventListener('message', (event) => {
        if (event.data.type === 'updateSchematic') loadSchematicData(event.data.data);
    });
    if (window.__BHDL_SCHEMATIC_DATA__) loadSchematicData(window.__BHDL_SCHEMATIC_DATA__);
    window.addEventListener('resize', resizeCanvas);
    resizeCanvas();
})();
