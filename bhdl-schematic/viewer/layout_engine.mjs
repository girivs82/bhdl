// layout_engine.mjs — Headless layout engine for BHDL schematics
//
// Runs the EXACT same computeLayout() code from schematic.js in a sandboxed
// Node.js context.  Zero code duplication — always produces the same layout
// as the browser viewer.
//
// Usage:
//   import { computeLayout } from './layout_engine.mjs';
//   const layout = computeLayout(schematicDataJSON);
//   // layout.elements — positioned component boxes with ports
//   // layout.wires    — routed wire segments
//   // layout.stageZones — stage zone bounding regions

import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import vm from 'vm';

const __dirname = dirname(fileURLToPath(import.meta.url));
const schematicJsPath = join(__dirname, 'schematic.js');

// ═══════════════════ SCRIPT PREPARATION ═══════════════════

let preparedScript = null;

function getScript() {
    if (preparedScript) return preparedScript;

    const source = readFileSync(schematicJsPath, 'utf8');

    // Unwrap the IIFE:  (function () { <body> })();
    const iifeMatch = source.match(/^\(function\s*\(\)\s*\{/m);
    if (!iifeMatch) throw new Error('Cannot find IIFE opening in schematic.js');
    const bodyStart = iifeMatch.index + iifeMatch[0].length;

    const bodyEnd = source.lastIndexOf('})();');
    if (bodyEnd < bodyStart) throw new Error('Cannot find IIFE closing in schematic.js');

    let body = source.substring(bodyStart, bodyEnd);

    // Append injection code that calls loadSchematicData and captures results.
    // This runs in the same lexical scope as the IIFE body, so it can read
    // the `let` variables (layoutElements, layoutWires, layoutStageZones).
    body += `
// ═══ layout_engine.mjs result capture ═══
if (__inputData) {
    loadSchematicData(__inputData);
    __output.elements = layoutElements;
    __output.wires = layoutWires;
    __output.stageZones = layoutStageZones;
}
`;

    preparedScript = new vm.Script(body, { filename: 'schematic-layout.js' });
    return preparedScript;
}

// ═══════════════════ MOCK BROWSER GLOBALS ═══════════════════

function createSandbox(inputData) {
    const noop = () => {};
    const output = { elements: null, wires: null, stageZones: null };

    // Minimal mock of the browser objects schematic.js touches at init time
    const mockCtx = {
        save: noop, restore: noop, setTransform: noop,
        fillStyle: '', strokeStyle: '', lineWidth: 0, globalAlpha: 1,
        textAlign: '', textBaseline: '', font: '',
        fillRect: noop, strokeRect: noop, clearRect: noop,
        beginPath: noop, closePath: noop, moveTo: noop, lineTo: noop,
        arc: noop, arcTo: noop, quadraticCurveTo: noop, bezierCurveTo: noop,
        stroke: noop, fill: noop,
        fillText: noop, strokeText: noop,
        measureText: (t) => ({ width: (t || '').length * 7 }),
        translate: noop, scale: noop, rotate: noop,
        setLineDash: noop, getLineDash: () => [],
        roundRect: noop,
        createLinearGradient: () => ({ addColorStop: noop }),
        drawImage: noop, getImageData: noop, putImageData: noop,
    };

    const mockElement = () => ({
        textContent: '', innerText: '', innerHTML: '',
        style: { display: '' },
        checked: false,
        value: '',
        addEventListener: noop,
        removeEventListener: noop,
        appendChild: noop,
        removeChild: noop,
        setAttribute: noop,
        getAttribute: () => null,
        classList: { add: noop, remove: noop, toggle: noop, contains: () => false },
        getBoundingClientRect: () => ({ left: 0, top: 0, width: 0, height: 0 }),
    });

    const sandbox = {
        // DOM mocks
        document: {
            getElementById: (id) => {
                if (id === 'schematic-canvas') {
                    return {
                        ...mockElement(),
                        clientWidth: 1920,
                        clientHeight: 1080,
                        width: 1920,
                        height: 1080,
                        getContext: () => mockCtx,
                    };
                }
                return mockElement();
            },
            createElement: () => mockElement(),
            querySelector: () => null,
            querySelectorAll: () => [],
        },
        window: {
            devicePixelRatio: 1,
            addEventListener: noop,
            removeEventListener: noop,
            requestAnimationFrame: noop,
            __BHDL_SCHEMATIC_DATA__: null, // don't auto-load
        },
        navigator: { userAgent: 'node' },

        // JS built-ins (from outer realm so instanceof checks work)
        Set, Map, Array, Object, Math, JSON, String, Number, Boolean, RegExp,
        Error, TypeError, RangeError, SyntaxError, ReferenceError, URIError,
        Date, Symbol, WeakMap, WeakSet, Promise, Proxy, Reflect,
        Infinity, NaN, undefined,
        parseInt, parseFloat, isNaN, isFinite,
        encodeURIComponent, decodeURIComponent, encodeURI, decodeURI,
        setTimeout: noop, setInterval: noop,
        clearTimeout: noop, clearInterval: noop,
        requestAnimationFrame: noop, cancelAnimationFrame: noop,
        console,
        structuredClone: typeof structuredClone !== 'undefined' ? structuredClone : undefined,

        // Injection points
        __inputData: inputData,
        __output: output,
    };

    return { context: vm.createContext(sandbox), output };
}

// ═══════════════════ PUBLIC API ═══════════════════

/**
 * Run the schematic layout engine on a SchematicData JSON object.
 *
 * @param {object} schematicData — the full SchematicData structure from Rust CLI
 * @returns {{ elements: Array, wires: Array, stageZones: Array }}
 */
export function computeLayout(schematicData) {
    // Deep clone so we don't mutate the caller's object
    // (loadSchematicData converts power_nets from array to Set, etc.)
    const data = JSON.parse(JSON.stringify(schematicData));

    const script = getScript();
    const { context, output } = createSandbox(data);

    script.runInContext(context, { timeout: 30000 });

    return {
        elements: output.elements || [],
        wires: output.wires || [],
        stageZones: output.stageZones || [],
    };
}

// Re-export constants that validators might need
export const LAYOUT_CONSTANTS = {
    PORT_SPACING: 24,
    PORT_STUB_LEN: 14,
    BASE_GAP: 44,
    MAX_GAP: 200,
    ROW_GAP: 50,
    HEADER_HEIGHT: 22,
    FONT_SIZE: 11,
    PORT_DOT_R: 3,
    ENTITY_BOX_MIN_WIDTH: 180,
    INSTANCE_BOX_MIN_WIDTH: 120,
    GND_STUB_HEIGHT: 18,
};
