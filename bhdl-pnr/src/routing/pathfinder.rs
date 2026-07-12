//! PathFinder negotiated congestion router.
//!
//! Adapted from McMurchie & Ebeling (1995) for 3D PCB routing grid.
//! Each iteration: route all nets (allowing overlaps), then increase
//! cost of congested resources. Nets "negotiate" for resources until
//! no congestion remains.

use crate::routing::grid::{CellCoord, RoutingGrid};
use crate::types::*;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// Route all nets using PathFinder negotiated congestion.
///
/// When `allow_vias` is false, routing is confined to each pin's starting
/// layer (single-layer routing). Set to true for multi-layer routing.
pub fn pathfinder_route(
    grid: &mut RoutingGrid,
    nets: &[PnrNet],
    board: &Board,
    max_iterations: usize,
    history_factor: f64,
    present_factor: f64,
    allow_vias: bool,
) -> Vec<Route> {
    let mut routes: Vec<Route> = nets.iter().map(|n| Route::empty(n.id)).collect();

    // Sort nets by priority (high effective-weight first). Effective
    // weight = base net weight + constraint criticality, so diff pairs /
    // impedance-controlled / clock / length-matched nets route first and
    // get the least-congested paths (negotiated-congestion routers are
    // order-sensitive). Un-annotated boards see no change (zero bonus).
    let crit = crate::routing::criticality::net_criticality(board);
    let mut net_order: Vec<usize> = (0..nets.len()).collect();
    net_order.sort_by(|&a, &b| {
        let wa = crate::routing::criticality::effective_weight(&nets[a], &crit);
        let wb = crate::routing::criticality::effective_weight(&nets[b], &crit);
        wb.partial_cmp(&wa).unwrap_or(Ordering::Equal)
    });

    let comp_idx: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    for _iteration in 0..max_iterations {
        grid.reset_demand();

        for &net_idx in &net_order {
            let net = &nets[net_idx];
            if net.pins.len() < 2 {
                continue;
            }

            // Power/ground nets use copper planes when dedicated layers exist
            if net.is_plane_connected(&board.layer_stack) {
                continue;
            }

            // Rip up previous route
            if !routes[net_idx].is_empty() {
                remove_route_demand(grid, &routes[net_idx]);
            }

            // Find shortest path with congestion-aware cost
            let route = shortest_path_3d(
                grid,
                net,
                board,
                &comp_idx,
                history_factor,
                present_factor,
                allow_vias,
            );

            // Add route demand
            add_route_demand(grid, &route);
            routes[net_idx] = route;
        }

        // Update history for overused cells. Skip blocked cells (cap 0): the
        // only demand they carry is a net terminating on its own pin terminal,
        // which is unavoidable and must not accrue congestion history.
        for layer in &mut grid.cells {
            for row in layer {
                for cell in row {
                    if cell.blocked {
                        continue;
                    }
                    if cell.demand > cell.capacity {
                        cell.history += (cell.demand - cell.capacity) as f64;
                    }
                }
            }
        }

        // Check convergence
        if grid.max_overflow() == 0 {
            break;
        }
    }

    // Negotiation may hit max_iterations with residual overflow. Copper
    // that crosses another net must NEVER ship: rip the lowest-priority
    // routes occupying overused cells until the board is legal. An
    // unrouted net is an honest, visible failure (metrics + the
    // oracle's unconnected list); an illegal track is a silent lie.
    if grid.max_overflow() > 0 {
        for &net_idx in net_order.iter().rev() {
            if grid.max_overflow() == 0 {
                break;
            }
            if routes[net_idx].is_empty() {
                continue;
            }
            let occupies_overuse = routes[net_idx].segments.iter().any(|seg| {
                let a = grid.point_to_cell(seg.start.0, seg.start.1, seg.layer);
                let b = grid.point_to_cell(seg.end.0, seg.end.1, seg.layer);
                grid.cells_between(a, b).iter().any(|c| {
                    let cell = grid.get(*c);
                    !cell.blocked && cell.demand > cell.capacity
                })
            });
            if occupies_overuse {
                remove_route_demand(grid, &routes[net_idx]);
                let net_name = nets[net_idx].name.clone();
                log::warn!(
                    "pathfinder: ripping unconverged net '{net_name}' — residual \
                     overflow after {max_iterations} iterations (unrouted beats illegal)"
                );
                routes[net_idx] = Route::empty(routes[net_idx].net_id);
            }
        }
    }

    routes
}

/// Dijkstra shortest path on 3D grid with congestion-aware cost.
fn shortest_path_3d(
    grid: &RoutingGrid,
    net: &PnrNet,
    board: &Board,
    comp_idx: &HashMap<ComponentId, usize>,
    history_factor: f64,
    present_factor: f64,
    allow_vias: bool,
) -> Route {
    if net.pins.len() < 2 {
        return Route::empty(net.id);
    }

    // Map pins to grid cells, keeping the EXACT pad coordinate per cell:
    // the fabricated track must land on pad copper, not the cell center
    // (a 1 mm cell center can sit off the pad entirely — KiCad's DRC
    // reads that as an unconnected track).
    let pin_targets: Vec<(CellCoord, (f64, f64))> = net
        .pins
        .iter()
        .filter_map(|&(comp_id, pin_id)| {
            let &ci = comp_idx.get(&comp_id)?;
            let comp = &board.components[ci];
            let pin = comp.pins.iter().find(|p| p.pin_id == pin_id)?;

            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;

            let layer = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => grid.num_layers - 1,
            };

            Some((grid.point_to_cell(gx, gy, layer), (gx, gy)))
        })
        .collect();
    let pin_cells: Vec<CellCoord> = pin_targets.iter().map(|(c, _)| *c).collect();

    if pin_cells.len() < 2 {
        return Route::empty(net.id);
    }

    // Multi-sink Dijkstra (Steiner tree approximation)
    let source = pin_cells[0];
    let mut remaining_sinks: HashSet<CellCoord> =
        pin_cells[1..].iter().cloned().collect();

    let mut all_segments = Vec::new();
    let mut all_vias = Vec::new();

    // Keep routing until all sinks reached (or we give up)
    let mut source_set: HashSet<CellCoord> = HashSet::new();
    source_set.insert(source);

    while !remaining_sinks.is_empty() {
        let result = dijkstra_to_any(
            grid,
            &source_set,
            &remaining_sinks,
            net,
            &board.layer_stack,
            history_factor,
            present_factor,
            allow_vias,
        );

        match result {
            Some((path, reached_sink)) => {
                remaining_sinks.remove(&reached_sink);
                // Add all cells in path to source set (Steiner approximation)
                for &cell in &path {
                    source_set.insert(cell);
                }
                // Convert path to segments + vias
                let (segs, vias) = path_to_segments(grid, &path, net.required_trace_width_mm);
                all_segments.extend(segs);
                all_vias.extend(vias);
            }
            None => break, // Unroutable — stop
        }
    }

    // Pad-escape stubs: connect each routed terminal's cell center to
    // the exact pad coordinate so the copper actually touches the pad.
    for (cell, (px, py)) in &pin_targets {
        if !source_set.contains(cell) {
            continue; // pin never joined the tree (unrouted) — no stub
        }
        let (cx, cy) = grid.cell_center(*cell);
        if (cx - px).hypot(cy - py) > 1e-6 {
            all_segments.push(RouteSegment {
                layer: cell.layer,
                start: (cx, cy),
                end: (*px, *py),
                width_mm: net.required_trace_width_mm,
            });
        }
    }

    Route {
        net_id: net.id,
        segments: all_segments,
        vias: all_vias,
    }
}

/// Dijkstra from any source cell to any sink cell.
fn dijkstra_to_any(
    grid: &RoutingGrid,
    sources: &HashSet<CellCoord>,
    sinks: &HashSet<CellCoord>,
    net: &PnrNet,
    stack: &LayerStack,
    history_factor: f64,
    present_factor: f64,
    allow_vias: bool,
) -> Option<(Vec<CellCoord>, CellCoord)> {
    let mut dist: HashMap<CellCoord, f64> = HashMap::new();
    let mut prev: HashMap<CellCoord, CellCoord> = HashMap::new();
    let mut heap = BinaryHeap::new();

    for &src in sources {
        dist.insert(src, 0.0);
        heap.push(DijkState {
            cost: 0.0,
            cell: src,
        });
    }

    while let Some(DijkState { cost, cell }) = heap.pop() {
        // Check if we reached a sink
        if sinks.contains(&cell) {
            // Backtrace
            let mut path = vec![cell];
            let mut cur = cell;
            while let Some(&p) = prev.get(&cur) {
                path.push(p);
                cur = p;
            }
            path.reverse();
            return Some((path, cell));
        }

        // Skip if we already found a better path
        if let Some(&d) = dist.get(&cell) {
            if cost > d {
                continue;
            }
        }

        // Expand planar neighbors (4 cardinal + 4 diagonal)
        for (nbr, move_cost) in grid.planar_neighbors(cell) {
            let gc = grid.get(nbr);
            // Blocked cells are unroutable through-traffic (pads, keepouts).
            // EXCEPTION: a cell that is one of *this* net's own terminals
            // must remain reachable — a pin's pad center can fall inside its
            // own pad-keepaway block (whether it does is a sub-cell alignment
            // accident vs. the routing grid), and a blocked sink that can
            // never be pushed onto the heap would make the net spuriously
            // unroutable. Terminals only; we still never tunnel through other
            // components' pads.
            if gc.blocked
                    && !sinks.contains(&nbr)
                    && gc.owner != Some(net.id)
                {
                continue;
            }

            let edge_cost = cell_cost(gc, history_factor, present_factor);
            let width_factor = (net.required_trace_width_mm / 0.2).max(1.0);
            let new_cost = cost + edge_cost * width_factor * move_cost;

            if new_cost < *dist.get(&nbr).unwrap_or(&f64::INFINITY) {
                dist.insert(nbr, new_cost);
                prev.insert(nbr, cell);
                heap.push(DijkState {
                    cost: new_cost,
                    cell: nbr,
                });
            }
        }

        // Expand vertical neighbors (via) — only when vias are allowed
        if allow_vias {
            for nbr in grid.vertical_neighbors(cell) {
                if !net.layer_constraint.allows(nbr.layer, stack) {
                    continue;
                }
                let gc = grid.get(nbr);
                // Same terminal exception as the planar case: a via may land
                // on a blocked cell only if it is this net's own pin terminal.
                if gc.blocked
                    && !sinks.contains(&nbr)
                    && gc.owner != Some(net.id)
                {
                    continue;
                }

                let new_cost = cost + grid.via_cost;
                if new_cost < *dist.get(&nbr).unwrap_or(&f64::INFINITY) {
                    dist.insert(nbr, new_cost);
                    prev.insert(nbr, cell);
                    heap.push(DijkState {
                        cost: new_cost,
                        cell: nbr,
                    });
                }
            }
        }
    }

    None // Unroutable
}

fn cell_cost(cell: &crate::routing::grid::GridCell, history_factor: f64, present_factor: f64) -> f64 {
    let base = 1.0;
    let history = cell.history * history_factor;
    let present = if cell.demand >= cell.capacity && cell.capacity > 0 {
        present_factor * (cell.demand - cell.capacity + 1) as f64
    } else {
        0.0
    };
    base + history + present
}

/// Convert a cell path to physical route segments and vias.
fn path_to_segments(
    grid: &RoutingGrid,
    path: &[CellCoord],
    width_mm: f64,
) -> (Vec<RouteSegment>, Vec<RouteVia>) {
    let mut segments = Vec::new();
    let mut vias = Vec::new();

    for window in path.windows(2) {
        let a = window[0];
        let b = window[1];
        let (ax, ay) = grid.cell_center(a);
        let (bx, by) = grid.cell_center(b);

        if a.layer != b.layer {
            // Via
            vias.push(RouteVia {
                x: ax,
                y: ay,
                from_layer: a.layer,
                to_layer: b.layer,
            });
        } else {
            // Trace segment
            segments.push(RouteSegment {
                layer: a.layer,
                start: (ax, ay),
                end: (bx, by),
                width_mm,
            });
        }
    }

    (segments, vias)
}

fn add_route_demand(grid: &mut RoutingGrid, route: &Route) {
    for cell in route_cells(grid, route) {
        grid.get_mut(cell).demand += 1;
    }
    for via in &route.vias {
        let from = grid.point_to_cell(via.x, via.y, via.from_layer);
        let to = grid.point_to_cell(via.x, via.y, via.to_layer);
        grid.get_mut(from).demand += 1;
        grid.get_mut(to).demand += 1;
    }
}

/// Every cell a route occupies or grazes, each counted ONCE per route:
/// the path cells plus, for DIAGONAL steps, the two cardinal companion
/// cells the diagonal geometrically crosses. Without companions a
/// second net can route the opposite diagonal through the same
/// cell-pair square — a physical X the per-cell model can't see (the
/// oracle's tracks_crossing family). Deduped per route because a net's
/// own zigzag grazes the same companion twice — a net cannot conflict
/// with itself, and double-charging made the ripper rip its own route.
pub(crate) fn route_cells(grid: &RoutingGrid, route: &Route) -> std::collections::HashSet<CellCoord> {
    let mut cells = std::collections::HashSet::new();
    for seg in &route.segments {
        let start = grid.point_to_cell(seg.start.0, seg.start.1, seg.layer);
        let end = grid.point_to_cell(seg.end.0, seg.end.1, seg.layer);
        let path = grid.cells_between(start, end);
        for c in &path {
            cells.insert(*c);
        }
        for w in path.windows(2) {
            for comp in diagonal_companions(w[0], w[1]) {
                cells.insert(comp);
            }
        }
    }
    cells
}

/// The two cardinal cells a diagonal step grazes; empty for cardinal steps.
fn diagonal_companions(a: CellCoord, b: CellCoord) -> Vec<CellCoord> {
    if a.layer != b.layer || a.row == b.row || a.col == b.col {
        return Vec::new();
    }
    vec![
        CellCoord { layer: a.layer, row: a.row, col: b.col },
        CellCoord { layer: a.layer, row: b.row, col: a.col },
    ]
}

fn remove_route_demand(grid: &mut RoutingGrid, route: &Route) {
    for cell in route_cells(grid, route) {
        let c = grid.get_mut(cell);
        c.demand = c.demand.saturating_sub(1);
    }
    for via in &route.vias {
        let from = grid.point_to_cell(via.x, via.y, via.from_layer);
        let to = grid.point_to_cell(via.x, via.y, via.to_layer);
        let f = grid.get_mut(from);
        f.demand = f.demand.saturating_sub(1);
        let t = grid.get_mut(to);
        t.demand = t.demand.saturating_sub(1);
    }
}

// ── Dijkstra priority queue ────────────────────────────────────────────

struct DijkState {
    cost: f64,
    cell: CellCoord,
}

impl PartialEq for DijkState {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost
    }
}

impl Eq for DijkState {}

impl PartialOrd for DijkState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DijkState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}
