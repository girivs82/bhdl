#!/usr/bin/env node
/**
 * Convert tscircuit/schematic-symbols JSON data to compact JS object for embedding.
 *
 * Downloads all available symbol JSONs from the tscircuit GitHub repo,
 * preprocesses them (negate Y for Canvas, strip metadata, compact format),
 * and outputs a JS const block ready to paste into schematic.js.
 *
 * Usage: node convert_tscircuit_symbols.mjs > tscircuit_symbols.js
 */
import fs from 'fs';

const BASE_URL = 'https://raw.githubusercontent.com/tscircuit/schematic-symbols/main/assets/generated/';

// Complete list of all 89 symbol names from tscircuit/schematic-symbols assets/generated/
const SYMBOL_NAMES = [
  // Passives
  'resistor', 'boxresistor', 'boxresistor_small', 'capacitor', 'capacitor_polarized',
  'inductor', 'ferrite_bead', 'varistor', 'light_dependent_resistor',
  'potentiometer', 'potentiometer2', 'potentiometer3',
  'crystal', 'crystal_4pin', 'resonator',
  // Diodes
  'diode', 'filled_diode', 'rectifier_diode', 'schottky_diode', 'zener_diode',
  'led', 'icled', 'photodiode', 'laser_diode',
  'avalanche_diode', 'backward_diode', 'tunnel_diode', 'varactor_diode',
  'constant_current_diode', 'gunn_diode', 'step_recovery_diode',
  // Transistors
  'npn_bipolar_transistor', 'pnp_bipolar_transistor',
  'n_channel_e_mosfet_transistor', 'p_channel_e_mosfet_transistor',
  'n_channel_d_mosfet_transistor', 'p_channel_d_mosfet_transistor',
  'mosfet_depletion_normally_on',
  'igbt_transistor', 'darlington_pair_transistor',
  'unijunction_transistor', 'njfet_transistor', 'pjfet_transistor',
  // Thyristors
  'diac', 'triac', 'silicon_controlled_rectifier',
  // Power
  'battery', 'current_source',
  // Switches
  'spst_switch', 'spst_normally_closed_switch',
  'spdt_switch', 'spdt_normally_closed_switch',
  'dpst_switch', 'dpst_normally_closed_switch',
  'dpdt_switch', 'dpdt_normally_closed_switch',
  'push_button_normally_open_momentary', 'push_button_normally_closed_momentary',
  'illuminated_push_button_normally_open',
  'mushroom_head_normally_open_momentary',
  // Meters
  'ac_voltmeter', 'dc_ammeter', 'dc_voltmeter', 'volt_meter',
  'wattmeter', 'watt_hour_meter', 'var_meter', 'varmeter',
  'frequency_meter', 'power_factor_meter', 'tachometer',
  // Ground/power symbols
  'ground', 'digital_ground', 'bridged_ground', 'tilted_ground',
  'vcc', 'rail',
  // Op-amps
  'opamp_no_power', 'opamp_with_power',
  // Solder jumpers
  'solderjumper2', 'solderjumper2_bridged12',
  'solderjumper3', 'solderjumper3_bridged12', 'solderjumper3_bridged123', 'solderjumper3_bridged23',
  // Misc
  'usbc', 'not_connected', 'square_wave', 'fuse',
];

async function fetchSymbol(name) {
  const url = `${BASE_URL}${name}.json`;
  try {
    const res = await fetch(url);
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  }
}

function round4(n) {
  return Math.round(n * 10000) / 10000;
}

function processSymbol(data) {
  // tscircuit format: paths is an OBJECT with named keys,
  // each entry has type: "path" or "circle"
  const pathsOut = [];
  const circlesOut = [];
  const refblocks = {};

  // Process paths object (contains both path and circle primitives)
  for (const [key, prim] of Object.entries(data.paths || {})) {
    if (prim.type === 'path' && prim.points) {
      const points = prim.points.map(pt => [round4(pt.x), round4(-pt.y)]);
      pathsOut.push({ p: points, f: prim.fill || false });
    } else if (prim.type === 'circle') {
      circlesOut.push([round4(prim.x), round4(-prim.y), round4(prim.radius)]);
    }
  }

  // Extract refblock positions
  for (const [key, val] of Object.entries(data.refblocks || {})) {
    refblocks[key] = [round4(val.x), round4(-val.y)];
  }

  // Use bounds from the JSON if available, else compute from paths
  let w, h;
  if (data.bounds && data.bounds.width != null) {
    w = round4(data.bounds.width);
    h = round4(data.bounds.height);
  } else {
    // Fallback: compute from path extents
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const path of pathsOut) {
      for (const [x, y] of path.p) {
        minX = Math.min(minX, x); minY = Math.min(minY, y);
        maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
      }
    }
    w = round4(maxX - minX);
    h = round4(maxY - minY);
  }

  const result = { paths: pathsOut, bounds: { w, h } };
  if (circlesOut.length > 0) result.circles = circlesOut;

  // Attach refblock positions with ref_ prefix
  for (const [key, pos] of Object.entries(refblocks)) {
    result['ref_' + key] = pos;
  }

  return result;
}

async function main() {
  const symbols = {};
  let fetched = 0;
  let failed = 0;

  for (const name of SYMBOL_NAMES) {
    const data = await fetchSymbol(name);
    if (data) {
      const processed = processSymbol(data);
      if (processed.paths.length > 0 || (processed.circles && processed.circles.length > 0)) {
        symbols[name] = processed;
        fetched++;
        process.stderr.write(`  OK: ${name} (${processed.paths.length} paths, ${(processed.circles || []).length} circles)\n`);
      } else {
        failed++;
        process.stderr.write(`  EMPTY: ${name} (no drawing data)\n`);
      }
    } else {
      failed++;
      process.stderr.write(`  SKIP: ${name} (not found)\n`);
    }
  }

  process.stderr.write(`\nFetched ${fetched} symbols, skipped ${failed}\n`);

  // Output as JS const — one symbol per line for reasonable readability + size
  console.log('    // Symbol path data from tscircuit/schematic-symbols');
  console.log('    // Copyright 2024 tscircuit contributors — MIT License');
  console.log('    // https://github.com/tscircuit/schematic-symbols');
  console.log('    // Auto-generated by convert_tscircuit_symbols.mjs — do not edit manually');
  console.log('    const TSCIRCUIT_SYMBOLS = {');
  const entries = Object.entries(symbols);
  for (let i = 0; i < entries.length; i++) {
    const [name, sym] = entries[i];
    const comma = i < entries.length - 1 ? ',' : '';
    console.log(`      ${JSON.stringify(name)}: ${JSON.stringify(sym)}${comma}`);
  }
  console.log('    };');
}

main().catch(e => { console.error(e); process.exit(1); });
