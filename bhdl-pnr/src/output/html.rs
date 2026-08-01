//! Interactive HTML board visualization.
//!
//! Generates a standalone HTML file with Canvas rendering showing:
//! - Board outline
//! - Component footprints (colored by type, labeled with refdes)
//! - Routed traces (colored by layer)
//! - Vias
//! - Functional group boundaries
//! - Net names on hover

use crate::ipc7351;
use crate::types::*;

/// Export board and routes to a standalone interactive HTML file.
pub fn export_html(board: &Board, routes: &[Route], metrics: &PnrMetrics) -> String {
    let w = board.config.outline.width();
    let h = board.config.outline.height();

    // Serialize board data to JSON for the JS renderer
    let components_json = board_components_json(board);
    let routes_json = routes_to_json(board, routes);
    let groups_json = groups_to_json(board);
    let nets_json = nets_to_json(board);

    format!(
        r##"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>BHDL PCB Layout</title>
<style>
  body {{ margin: 0; background: #1a1a2e; color: #eee; font-family: 'Segoe UI', system-ui, sans-serif; }}
  #toolbar {{ padding: 8px 16px; background: #16213e; display: flex; gap: 16px; align-items: center; font-size: 13px; }}
  #toolbar .metric {{ background: #0f3460; padding: 4px 10px; border-radius: 4px; }}
  #toolbar .metric b {{ color: #e94560; }}
  #info {{ position: fixed; bottom: 8px; left: 8px; font-size: 12px; opacity: 0.6; }}
  canvas {{ display: block; cursor: crosshair; }}
</style>
</head>
<body>
<div id="toolbar">
  <span style="font-weight:bold; color:#e94560;">BHDL PCB Layout</span>
  <span class="metric">HPWL: <b>{hpwl:.1}</b> mm</span>
  <span class="metric">Routed: <b>{routed:.1}</b> mm</span>
  <span class="metric">Vias: <b>{vias}</b></span>
  <span class="metric">Routability: <b>{routability:.0}%</b></span>
  <span class="metric">DRC: <b>{drc}</b></span>
  <span class="metric">Board: <b>{bw:.1} × {bh:.1}</b> mm</span>
</div>
<canvas id="c"></canvas>
<div id="info">Scroll to zoom · Drag to pan · Hover for info</div>
<script>
"use strict";
const BOARD_W = {bw};
const BOARD_H = {bh};
const COMPONENTS = {components_json};
const ROUTES = {routes_json};
const GROUPS = {groups_json};
const NETS = {nets_json};

const canvas = document.getElementById('c');
const ctx = canvas.getContext('2d');

let scale = 1, offsetX = 0, offsetY = 0;
let isDragging = false, dragStartX = 0, dragStartY = 0;
let hoverComp = null;

function resize() {{
  canvas.width = window.innerWidth;
  canvas.height = window.innerHeight - 36;
  const sx = (canvas.width - 40) / BOARD_W;
  const sy = (canvas.height - 40) / BOARD_H;
  scale = Math.min(sx, sy);
  offsetX = (canvas.width - BOARD_W * scale) / 2;
  offsetY = (canvas.height - BOARD_H * scale) / 2;
  draw();
}}

function toScreen(x, y) {{ return [offsetX + x * scale, offsetY + y * scale]; }}
function toBoard(sx, sy) {{ return [(sx - offsetX) / scale, (sy - offsetY) / scale]; }}

const LAYER_COLORS = {{
  0: '#e94560',  // F.Cu — red
  1: '#4ecca3',  // In1 — teal
  2: '#f9a825',  // In2 — amber
  3: '#42a5f5',  // In3 — blue
  4: '#ab47bc',  // In4 — purple
  5: '#66bb6a',  // B.Cu — green
}};

const CAT_COLORS = {{
  resistor: '#8d6e63', capacitor: '#5c6bc0', inductor: '#26a69a',
  diode: '#ef5350', led: '#ffee58', voltage_regulator: '#42a5f5',
  ic: '#7e57c2', opamp: '#7e57c2', tvs_diode: '#ff7043',
  connector: '#78909c', ferrite_bead: '#8d6e63',
}};

function compColor(comp) {{
  return CAT_COLORS[comp.cat] || '#9e9e9e';
}}

function draw() {{
  ctx.clearRect(0, 0, canvas.width, canvas.height);

  // Board outline
  const [bx, by] = toScreen(0, 0);
  ctx.strokeStyle = '#e94560';
  ctx.lineWidth = 2;
  ctx.strokeRect(bx, by, BOARD_W * scale, BOARD_H * scale);

  // Grid (1mm)
  ctx.strokeStyle = 'rgba(255,255,255,0.04)';
  ctx.lineWidth = 0.5;
  for (let x = 0; x <= BOARD_W; x += 1) {{
    const [sx] = toScreen(x, 0);
    ctx.beginPath(); ctx.moveTo(sx, by); ctx.lineTo(sx, by + BOARD_H * scale); ctx.stroke();
  }}
  for (let y = 0; y <= BOARD_H; y += 1) {{
    const [, sy] = toScreen(0, y);
    ctx.beginPath(); ctx.moveTo(bx, sy); ctx.lineTo(bx + BOARD_W * scale, sy); ctx.stroke();
  }}

  // Functional groups (background)
  for (const g of GROUPS) {{
    if (g.members.length < 2) continue;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const idx of g.members) {{
      const c = COMPONENTS[idx];
      if (!c) continue;
      minX = Math.min(minX, c.x - c.w/2);
      minY = Math.min(minY, c.y - c.h/2);
      maxX = Math.max(maxX, c.x + c.w/2);
      maxY = Math.max(maxY, c.y + c.h/2);
    }}
    const [sx, sy] = toScreen(minX - 1, minY - 1);
    const sw = (maxX - minX + 2) * scale;
    const sh = (maxY - minY + 2) * scale;
    ctx.fillStyle = 'rgba(76, 175, 80, 0.08)';
    ctx.fillRect(sx, sy, sw, sh);
    ctx.strokeStyle = 'rgba(76, 175, 80, 0.3)';
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 4]);
    ctx.strokeRect(sx, sy, sw, sh);
    ctx.setLineDash([]);
    ctx.fillStyle = 'rgba(76, 175, 80, 0.5)';
    ctx.font = `${{Math.max(9, 10 * scale / 8)}}px sans-serif`;
    ctx.fillText(g.name, sx + 3, sy - 3);
  }}

  // Routes
  for (const r of ROUTES) {{
    for (const seg of r.segments) {{
      const [x1, y1] = toScreen(seg.x1, seg.y1);
      const [x2, y2] = toScreen(seg.x2, seg.y2);
      ctx.strokeStyle = LAYER_COLORS[seg.layer] || '#888';
      ctx.lineWidth = Math.max(1, seg.width * scale);
      ctx.globalAlpha = 0.7;
      ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
      ctx.globalAlpha = 1;
    }}
    for (const v of r.vias) {{
      const [vx, vy] = toScreen(v.x, v.y);
      const vr = Math.max(2, 0.3 * scale);
      ctx.fillStyle = '#fff';
      ctx.beginPath(); ctx.arc(vx, vy, vr, 0, Math.PI * 2); ctx.fill();
      ctx.strokeStyle = '#e94560';
      ctx.lineWidth = 1;
      ctx.stroke();
    }}
  }}

  // Components
  for (let i = 0; i < COMPONENTS.length; i++) {{
    const c = COMPONENTS[i];
    const [cx, cy] = toScreen(c.x, c.y);
    const sw = c.w * scale;
    const sh = c.h * scale;

    ctx.save();
    ctx.translate(cx, cy);
    ctx.rotate(c.theta);

    // Body
    const color = compColor(c);
    ctx.fillStyle = (hoverComp === i) ? '#fff' : color;
    ctx.globalAlpha = (hoverComp === i) ? 0.3 : 0.6;
    ctx.fillRect(-sw/2, -sh/2, sw, sh);
    ctx.globalAlpha = 1;
    ctx.strokeStyle = (hoverComp === i) ? '#fff' : color;
    ctx.lineWidth = (hoverComp === i) ? 2 : 1;
    ctx.strokeRect(-sw/2, -sh/2, sw, sh);

    // Pads (actual IPC-7351B dimensions)
    ctx.fillStyle = '#ffd54f';
    for (const pin of c.pins) {{
      const px = pin.dx * scale;
      const py = pin.dy * scale;
      const pw = Math.max(1.5, (pin.pw || 0.5) * scale);
      const ph = Math.max(1.5, (pin.ph || 0.3) * scale);
      ctx.fillRect(px - pw/2, py - ph/2, pw, ph);
    }}

    // Label
    if (scale > 4) {{
      ctx.fillStyle = '#fff';
      ctx.font = `${{Math.max(8, 1.5 * scale)}}px monospace`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(c.refdes, 0, 0);
    }} else if (scale > 2) {{
      ctx.fillStyle = '#fff';
      ctx.font = '8px monospace';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'bottom';
      ctx.fillText(c.refdes, 0, -sh/2 - 2);
    }}

    ctx.restore();
  }}

  // Hover tooltip
  if (hoverComp !== null) {{
    const c = COMPONENTS[hoverComp];
    const [tx, ty] = toScreen(c.x, c.y);
    const lines = [
      `${{c.refdes}} (${{c.name}})`,
      `${{c.pkg}} ${{c.w.toFixed(1)}}×${{c.h.toFixed(1)}}mm`,
      `${{c.pins.length}} pins`,
      c.intent ? `intent: ${{c.intent}}` : '',
      c.group ? `group: ${{c.group}}` : '',
    ].filter(Boolean);
    const tw = Math.max(...lines.map(l => ctx.measureText(l).width)) + 16;
    const th = lines.length * 16 + 8;
    const ttx = Math.min(tx + 10, canvas.width - tw - 4);
    const tty = Math.min(ty + 10, canvas.height - th - 4);
    ctx.fillStyle = 'rgba(0,0,0,0.85)';
    ctx.fillRect(ttx, tty, tw, th);
    ctx.fillStyle = '#fff';
    ctx.font = '12px monospace';
    ctx.textAlign = 'left';
    lines.forEach((l, i) => ctx.fillText(l, ttx + 8, tty + 16 + i * 16));
  }}
}}

// Hit test
function hitTest(mx, my) {{
  const [bx, by] = toBoard(mx, my);
  for (let i = COMPONENTS.length - 1; i >= 0; i--) {{
    const c = COMPONENTS[i];
    if (Math.abs(bx - c.x) < c.w/2 + 0.5 && Math.abs(by - c.y) < c.h/2 + 0.5) return i;
  }}
  return null;
}}

canvas.addEventListener('mousemove', e => {{
  if (isDragging) {{
    offsetX += e.clientX - dragStartX;
    offsetY += e.clientY - dragStartY;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    draw();
  }} else {{
    const prev = hoverComp;
    hoverComp = hitTest(e.clientX, e.clientY - 36);
    if (hoverComp !== prev) draw();
  }}
}});

canvas.addEventListener('mousedown', e => {{
  isDragging = true; dragStartX = e.clientX; dragStartY = e.clientY;
}});
canvas.addEventListener('mouseup', () => {{ isDragging = false; }});
canvas.addEventListener('mouseleave', () => {{ isDragging = false; }});

canvas.addEventListener('wheel', e => {{
  e.preventDefault();
  const [bx, by] = toBoard(e.clientX, e.clientY - 36);
  const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
  scale *= factor;
  offsetX = e.clientX - bx * scale;
  offsetY = (e.clientY - 36) - by * scale;
  draw();
}}, {{ passive: false }});

window.addEventListener('resize', resize);
resize();
</script>
</body>
</html>"##,
        hpwl = metrics.hpwl_mm,
        routed = metrics.total_routed_length_mm,
        vias = metrics.via_count,
        routability = metrics.routability_pct,
        drc = 0, // placeholder
        bw = w,
        bh = h,
        components_json = components_json,
        routes_json = routes_json,
        groups_json = groups_json,
        nets_json = nets_json,
    )
}

// ── JSON serialization helpers ───────────────────────────────────────

fn board_components_json(board: &Board) -> String {
    let mut items = Vec::new();
    for comp in &board.components {
        let cat = categorize_for_color(&comp.package, &comp.name);
        let intent = "null";
        let group = comp.group.map(|g| {
            board.groups.iter()
                .find(|fg| fg.id == g)
                .map(|fg| format!("\"{}\"", fg.name))
                .unwrap_or_else(|| "null".to_string())
        }).unwrap_or_else(|| "null".to_string());

        // Get pad dimensions from IPC footprint (or default 0.5×0.3mm)
        let footprint = ipc7351::standard_package(&comp.package)
            .map(|f| ipc7351::generate_footprint(&f, ipc7351::DensityLevel::Nominal));
        let pins_json: Vec<String> = comp.pins.iter()
            .enumerate()
            .map(|(i, p)| {
                let (pw, ph) = footprint.as_ref()
                    .and_then(|fp| fp.pads.get(i))
                    .map(|pad| (pad.width, pad.height))
                    .unwrap_or((0.5, 0.3));
                format!("{{\"dx\":{:.3},\"dy\":{:.3},\"pw\":{:.3},\"ph\":{:.3},\"name\":\"{}\"}}", p.dx, p.dy, pw, ph, p.name)
            })
            .collect();

        items.push(format!(
            "{{\"x\":{:.3},\"y\":{:.3},\"w\":{:.3},\"h\":{:.3},\"theta\":{:.4},\"refdes\":\"{}\",\"name\":\"{}\",\"pkg\":\"{}\",\"cat\":\"{}\",\"intent\":{},\"group\":{},\"pins\":[{}]}}",
            comp.x, comp.y, comp.width_mm, comp.height_mm, comp.theta,
            comp.refdes, comp.name, comp.package, cat, intent, group,
            pins_json.join(",")
        ));
    }
    format!("[{}]", items.join(",\n"))
}

fn routes_to_json(board: &Board, routes: &[Route]) -> String {
    let mut items = Vec::new();
    for route in routes {
        if route.is_empty() { continue; }
        let segs: Vec<String> = route.segments.iter()
            .map(|s| format!(
                "{{\"x1\":{:.3},\"y1\":{:.3},\"x2\":{:.3},\"y2\":{:.3},\"layer\":{},\"width\":{:.3}}}",
                s.start.0, s.start.1, s.end.0, s.end.1, s.layer, s.width_mm
            ))
            .collect();
        let vias: Vec<String> = route.vias.iter()
            .map(|v| format!("{{\"x\":{:.3},\"y\":{:.3}}}", v.x, v.y))
            .collect();
        items.push(format!("{{\"segments\":[{}],\"vias\":[{}]}}", segs.join(","), vias.join(",")));
    }
    format!("[{}]", items.join(",\n"))
}

fn groups_to_json(board: &Board) -> String {
    let comp_idx: crate::det::HashMap<ComponentId, usize> = board.components.iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    let items: Vec<String> = board.groups.iter()
        .map(|g| {
            let members: Vec<String> = g.members.iter()
                .filter_map(|id| comp_idx.get(id).map(|i| i.to_string()))
                .collect();
            format!("{{\"name\":\"{}\",\"members\":[{}]}}", g.name, members.join(","))
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn nets_to_json(board: &Board) -> String {
    let items: Vec<String> = board.nets.iter()
        .filter(|n| n.pins.len() >= 2)
        .map(|n| format!("{{\"name\":\"{}\",\"weight\":{:.1}}}", n.name, n.weight))
        .collect();
    format!("[{}]", items.join(","))
}

fn categorize_for_color(package: &str, name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("res") || package.starts_with("06") || package.starts_with("08") || package.starts_with("12") || package.starts_with("20") || package.starts_with("25") {
        if lower.contains("cap") { return "capacitor"; }
        if lower.contains("ind") || lower.contains("_l_") || lower.starts_with("l") { return "inductor"; }
        return "resistor";
    }
    if lower.contains("cap") || lower.contains("_c_") { return "capacitor"; }
    if lower.contains("ind") || lower.contains("_l_") { return "inductor"; }
    if lower.contains("led") { return "led"; }
    if lower.contains("diode") || lower.contains("tvs") || lower.contains("_d_") { return "diode"; }
    if lower.contains("reg") || lower.contains("ldo") || lower.contains("buck") || lower.contains("ap63") || lower.contains("tps5") || lower.contains("ap21") || lower.contains("xc62") { return "voltage_regulator"; }
    "ic"
}
