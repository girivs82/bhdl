//! PathFinder negotiated congestion router.
//!
//! Adapted from McMurchie & Ebeling (1995) for 3D PCB routing grid.
//! Each iteration: route all nets (allowing overlaps), then increase
//! cost of congested resources. Nets "negotiate" for resources until
//! no congestion remains.

use crate::routing::grid::{CellCoord, RoutingGrid};
use crate::types::*;
use std::cmp::Ordering;
use std::collections::{BTreeSet, BinaryHeap, HashMap};

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
    let mut remaining_sinks: BTreeSet<CellCoord> =
        pin_cells[1..].iter().cloned().collect();

    let mut all_segments = Vec::new();
    let mut all_vias = Vec::new();

    // Keep routing until all sinks reached (or we give up)
    let mut source_set: BTreeSet<CellCoord> = BTreeSet::new();
    source_set.insert(source);

    let total_sinks = remaining_sinks.len();
    let mut unreached = 0usize;
    // Per-path records for power-tree FLOW ANALYSIS: each Dijkstra path
    // attaches one sink to the tree; a segment's current is the sum of
    // the shares of every sink downstream of it. Sizing every segment
    // at the RAIL total (the old behavior) demanded absurd widths
    // (7.2mm for a 5A rail) that can never fit — real power trees
    // taper: trunk wide, leaves thin. Per-pin draws are approximated as
    // equal shares of the net current until per-pin solved currents are
    // plumbed through (documented approximation).
    struct PathRec {
        cells: Vec<CellCoord>,
        /// (parent path index, cell index within parent) where this
        /// path attached; None = attached at the source pin itself.
        parent: Option<(usize, usize)>,
    }
    let mut path_recs: Vec<PathRec> = Vec::new();
    let mut cell_home: std::collections::HashMap<CellCoord, (usize, usize)> =
        std::collections::HashMap::new();
    let mut via_keepout: Vec<(f64, f64)> = Vec::new();

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
            &via_keepout,
            &[],
        );

        match result {
            Some((path, reached_sink)) => {
                remaining_sinks.remove(&reached_sink);
                // Attachment point: the first path cell already in the
                // tree (paths start from the tree and end at the sink).
                let parent = path.first().and_then(|c| cell_home.get(c)).copied();
                let pidx = path_recs.len();
                for (ci, &cell) in path.iter().enumerate() {
                    source_set.insert(cell);
                    cell_home.entry(cell).or_insert((pidx, ci));
                }
                for w in path.windows(2) {
                    if w[0].layer != w[1].layer {
                        via_keepout.push(grid.cell_center(w[0]));
                    }
                }
                path_recs.push(PathRec { cells: path.clone(), parent });
            }
            // Multi-sink Dijkstra reaches ANY remaining sink — if it
            // returns None, NO remaining sink is reachable from the
            // tree, so there is nothing more to try this iteration.
            None => {
                unreached = remaining_sinks.len();
                // Frontier diagnosis: dump the wall around one
                // unreached sink — what blocks each neighbor and why.
                if std::env::var("BHDL_PNR_DEBUG_FRONTIER").is_ok() {
                    if let Some(sink) = remaining_sinks.iter().next() {
                        log::warn!(
                            "FRONTIER net '{}' sink {:?} (in tree: {}):",
                            net.name, sink, source_set.contains(sink)
                        );
                        for dr in -2i64..=2 {
                            let mut row = String::new();
                            for dc in -2i64..=2 {
                                let r = sink.row as i64 + dr;
                                let c = sink.col as i64 + dc;
                                let ch = if r < 0
                                    || c < 0
                                    || (r as usize) >= grid.rows()
                                    || (c as usize) >= grid.cols()
                                {
                                    'X' // off-board
                                } else {
                                    let cell = grid.get(CellCoord {
                                        layer: sink.layer,
                                        row: r as usize,
                                        col: c as usize,
                                    });
                                    if dr == 0 && dc == 0 {
                                        'S' // the sink itself
                                    } else if cell.hard {
                                        'H'
                                    } else if cell.nc_blocked {
                                        'N'
                                    } else if cell.blocked
                                        && cell.owners.contains(&net.id)
                                    {
                                        'o' // ours — passable
                                    } else if cell.blocked {
                                        'F' // foreign-owned or ownerless block
                                    } else if cell.demand >= cell.capacity {
                                        'd' // congested
                                    } else {
                                        '.' // free
                                    }
                                };
                                row.push(ch);
                            }
                            log::warn!("  {row}");
                        }
                        // Also: is the sink cell itself blocked and why?
                        let sc = grid.get(*sink);
                        log::warn!(
                            "  sink cell: blocked={} hard={} nc={} owners_has_net={} demand={}/{} layer={}",
                            sc.blocked, sc.hard, sc.nc_blocked,
                            sc.owners.contains(&net.id), sc.demand, sc.capacity, sink.layer
                        );
                    }
                }
                break;
            }
        }
    }
    if unreached > 0 && std::env::var("BHDL_PNR_DEBUG_NETS").is_ok() {
        log::warn!(
            "ROUTE '{}' width={:.2}mm: {}/{} sinks unreached (vias={})",
            net.name, net.required_trace_width_mm,
            unreached, total_sinks, allow_vias
        );
    }

    // Flow analysis → tapered widths. downstream[p][i] = sinks served
    // through path p at-or-after cell i = 1 (p's own sink) + every
    // descendant path attached to p at index ≥ i, transitively.
    let n_paths = path_recs.len();
    let mut downstream: Vec<Vec<usize>> = path_recs
        .iter()
        .map(|r| vec![1usize; r.cells.len().max(1)])
        .collect();
    // Children contribute to the parent prefix [0..=attach_idx].
    // Process children before parents is unnecessary if we iterate to a
    // fixpoint over the (acyclic, forward-attached) structure: children
    // always have HIGHER indices than their parents, so one reverse
    // pass suffices.
    for p in (0..n_paths).rev() {
        let served: usize = *downstream[p].first().unwrap_or(&1);
        if let Some((pp, pi)) = path_recs[p].parent {
            for i in 0..=pi.min(downstream[pp].len().saturating_sub(1)) {
                downstream[pp][i] += served;
            }
        }
    }
    let share = if total_sinks > 0 {
        // Net current back-derived from the rail width the classifier
        // computed; equal-share approximation per sink.
        crate::stackup::current_for_trace_width(net.required_trace_width_mm)
            / total_sinks as f64
    } else {
        0.0
    };
    let mut path_spans: Vec<(usize, usize)> = Vec::new();
    let mut path_parents: Vec<Option<usize>> = Vec::new();
    let mut via_spans: Vec<(usize, usize)> = Vec::new();
    // Stubs emitted inside their branch's span: each path's last cell is
    // its sink — the sink's pad-escape stub belongs to THAT branch, so
    // amputating the branch removes its stub with it (previously stubs
    // were separate spans and amputation left them dangling — the
    // oracle's track_dangling family).
    let mut stub_of: std::collections::BTreeMap<CellCoord, Vec<(f64, f64)>> =
        std::collections::BTreeMap::new();
    for (c, p) in &pin_targets {
        stub_of.entry(*c).or_default().push(*p);
    }
    for (p, rec) in path_recs.iter().enumerate() {
        let span_start = all_segments.len();
        let via_start = all_vias.len();
        for w in 0..rec.cells.len().saturating_sub(1) {
            let a = rec.cells[w];
            let b = rec.cells[w + 1];
            let seg_current = share * downstream[p][w + 1] as f64;
            let width = crate::stackup::trace_width_for_current(seg_current, 1.0, 10.0)
                .max(0.15)
                .min(net.required_trace_width_mm);
            let (segs, vias) =
                path_to_segments(grid, &[a, b], width);
            all_segments.extend(segs);
            all_vias.extend(vias);
        }
        if let Some(last) = rec.cells.last() {
            if let Some(stubs) = stub_of.remove(last) {
                let (cx, cy) = grid.cell_center(*last);
                for (px, py) in stubs {
                    if (cx - px).hypot(cy - py) > 1e-6 {
                        all_segments.push(RouteSegment {
                            layer: last.layer,
                            start: (cx, cy),
                            end: (px, py),
                            width_mm: crate::stackup::trace_width_for_current(share, 1.0, 10.0)
                                .max(0.15)
                                .min(net.required_trace_width_mm),
                        });
                    }
                }
            }
        }
        path_spans.push((span_start, all_segments.len() - span_start));
        path_parents.push(rec.parent.map(|(pp, _)| pp));
        via_spans.push((via_start, all_vias.len() - via_start));
    }

    // Remaining pad-escape stubs (the source pin, and any pin whose
    // cell joined the tree mid-path rather than as a path end): own
    // spans — they serve the shared trunk, not one branch.
    for (cell, stubs) in &stub_of {
        if !source_set.contains(cell) {
            continue; // pin never joined the tree (unrouted) — no stub
        }
        for (px, py) in stubs {
            let (cx, cy) = grid.cell_center(*cell);
            if (cx - px).hypot(cy - py) > 1e-6 {
                path_spans.push((all_segments.len(), 1));
                // Parent stays None: these stubs serve the shared trunk
                // (often span 0, the parent of most branches) — wiring
                // them into the cascade lets one early-span amputation
                // nuke the whole tree. Orphaned stubs are pruned
                // geometrically after amputation instead.
                path_parents.push(None);
                via_spans.push((all_vias.len(), 0));
                all_segments.push(RouteSegment {
                    layer: cell.layer,
                    start: (cx, cy),
                    end: (*px, *py),
                    // Escape stubs carry the pin's own share, not the
                    // rail trunk width.
                    width_mm: crate::stackup::trace_width_for_current(share, 1.0, 10.0)
                        .max(0.15)
                        .min(net.required_trace_width_mm),
                });
            }
        }
    }

    Route {
        net_id: net.id,
        segments: all_segments,
        vias: all_vias,
        path_spans,
        path_parents,
        via_spans,
    }
}

/// Dijkstra from any source cell to any sink cell.
fn dijkstra_to_any(
    grid: &RoutingGrid,
    sources: &BTreeSet<CellCoord>,
    sinks: &BTreeSet<CellCoord>,
    net: &PnrNet,
    stack: &LayerStack,
    history_factor: f64,
    present_factor: f64,
    allow_vias: bool,
    via_keepout: &[(f64, f64)],
    banned_sites: &[(f64, f64)],
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
                    && (gc.hard || gc.nc_blocked || !gc.owners.contains(&net.id))
                {
                continue;
            }
            // Banned sites: cells where a previous recovery round's
            // copper was amputated by the validator (dangling ends) —
            // re-routing through them deterministically re-creates the
            // same offender and ping-pongs to the round cap.
            if !banned_sites.is_empty() && !sinks.contains(&nbr) {
                let (bx, by) = grid.cell_center(nbr);
                if banned_sites
                    .iter()
                    .any(|&(kx, ky)| (bx - kx).hypot(by - ky) < 0.01)
                {
                    continue;
                }
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
                    && (gc.hard || gc.nc_blocked || !gc.owners.contains(&net.id))
                {
                    continue;
                }
                // Via siting: the barrel (via pad + hole clearance) is
                // wider than a trace, so the halo carved for traces
                // under-protects it — require a clear ring around the
                // via column on BOTH layers or the oracle reports
                // clearance/hole_clearance against adjacent foreign
                // pads.
                let ring_clear = [cell, nbr].iter().all(|&c| {
                    grid.ring8(c).into_iter().all(|pn| {
                        let g = grid.get(pn);
                        !(g.hard
                            || g.nc_blocked
                            || (g.blocked && !g.owners.contains(&net.id)))
                    })
                });
                if !ring_clear && !sinks.contains(&nbr) {
                    continue;
                }
                if !banned_sites.is_empty() && !sinks.contains(&nbr) {
                    let (bx, by) = grid.cell_center(nbr);
                    if banned_sites
                        .iter()
                        .any(|&(kx, ky)| (bx - kx).hypot(by - ky) < 0.01)
                    {
                        continue;
                    }
                }
                // Drilled-hole spacing: no new via within hole-to-hole
                // distance of one this route already committed (same-net
                // vias violate hole_to_hole just like foreign ones).
                let hole_gap = stack.via.drill_mm + 0.25;
                let (vx, vy) = grid.cell_center(cell);
                if via_keepout
                    .iter()
                    .any(|&(kx, ky)| (vx - kx).hypot(vy - ky) < hole_gap)
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
pub(crate) fn route_cells(grid: &RoutingGrid, route: &Route) -> BTreeSet<CellCoord> {
    let mut cells = BTreeSet::new();
    let pitch = if grid.x_coords.len() > 1 {
        grid.x_coords[1] - grid.x_coords[0]
    } else {
        0.3
    };
    for seg in &route.segments {
        // Width-aware footprint: a track wider than one cell claims the
        // ring of cells its copper (+ half the spacing rule) covers —
        // without this, a 1mm power track claimed only its center-line
        // cells and neighbors routed inside its actual copper.
        let extra = (((seg.width_mm / 2.0 + pitch / 2.0) / pitch) - 0.5).ceil() as i64;
        let extra = extra.max(0);
        let start = grid.point_to_cell(seg.start.0, seg.start.1, seg.layer);
        let end = grid.point_to_cell(seg.end.0, seg.end.1, seg.layer);
        let path = grid.cells_between(start, end);
        for c in &path {
            cells.insert(*c);
            if extra > 0 {
                for dr in -extra..=extra {
                    for dc in -extra..=extra {
                        let r = c.row as i64 + dr;
                        let co = c.col as i64 + dc;
                        if r >= 0
                            && co >= 0
                            && (r as usize) < grid.rows()
                            && (co as usize) < grid.cols()
                        {
                            cells.insert(CellCoord {
                                layer: c.layer,
                                row: r as usize,
                                col: co as usize,
                            });
                        }
                    }
                }
            }
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
        self.cost == other.cost && self.cell == other.cell
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
        // Reverse ordering for min-heap; equal costs tie-break on the
        // CELL so the heap pop order is fully deterministic — with
        // Ordering::Equal ties, BinaryHeap's pop among equal-cost
        // states depended on insertion history, which depended on
        // HashSet iteration order, which is randomized per process:
        // the root of the run-to-run layout nondeterminism.
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.cell.cmp(&self.cell))
    }
}


/// Extend a PARTIAL route to its unreached pins: the surviving tree's
/// cells seed the Dijkstra source set and only pins not already
/// touched are targeted (vias allowed). New branches append as spans
/// with no parent (attachment to existing spans isn't tracked — a
/// later amputation of an old span won't cascade into these, which is
/// conservative). Used by the geometric-recovery loop after subtree
/// amputation: rebuilding a 90%-good power tree from scratch loses to
/// extending it.
pub(crate) fn extend_route(
    grid: &mut RoutingGrid,
    net: &PnrNet,
    board: &Board,
    route: &mut Route,
    history_factor: f64,
    present_factor: f64,
    banned_via_sites: &[(f64, f64)],
    banned_sites: &[(f64, f64)],
    full_width: bool,
) -> usize {
    let comp_idx: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    // Pin cells + exact pad coordinates + pad half-extent.
    let pin_targets: Vec<(CellCoord, (f64, f64), f64)> = net
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
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.4);
            Some((grid.point_to_cell(gx, gy, layer), (gx, gy), half))
        })
        .collect();

    // Existing copper = source set; unreached pins = sinks. "Reached"
    // is GEOMETRY, not cell membership: route_cells includes a fat
    // trunk's width ring, and a pin cell covered by the ring can still
    // have no copper touching its pad (that unc was invisible here —
    // extension thought there was nothing to do).
    let mut source_set: BTreeSet<CellCoord> = route_cells(grid, route);
    if source_set.is_empty() {
        // From-scratch greedy mode: seed from the first pin and grow the
        // tree sink by sink — no negotiation, so a lone fat net on a
        // crowded grid can't overflow-rip itself the way the negotiated
        // reroute does.
        match pin_targets.first() {
            Some((c, _, _)) => {
                source_set.insert(*c);
            }
            None => return 0,
        }
    }
    let pad_connected = |px: f64, py: f64, half: f64| {
        route.segments.iter().any(|sg| {
            let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
            let len2 = dx * dx + dy * dy;
            let t = if len2 <= 1e-12 {
                0.0
            } else {
                (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / len2).clamp(0.0, 1.0)
            };
            let (cx, cy) = (sg.start.0 + t * dx, sg.start.1 + t * dy);
            (px - cx).hypot(py - cy) < sg.width_mm / 2.0 + half - 0.001
        }) || route
            .vias
            .iter()
            .any(|v| (v.x - px).hypot(v.y - py) < half + 0.15)
    };
    let mut remaining: BTreeSet<CellCoord> = pin_targets
        .iter()
        .filter(|(_, (px, py), half)| !pad_connected(*px, *py, *half))
        .map(|(c, _, _)| *c)
        .collect();
    let share_width = if full_width {
        // From-scratch trunks may end up carrying the whole rail; use
        // the IPC width for the full current rather than a leaf share.
        net.required_trace_width_mm
    } else {
        crate::stackup::trace_width_for_current(
            crate::stackup::current_for_trace_width(net.required_trace_width_mm)
                / net.pins.len().max(1) as f64,
            1.0,
            10.0,
        )
        .max(0.15)
        .min(net.required_trace_width_mm)
    };

    let mut reclaimed = 0usize;
    let n_missing = remaining.len();
    let mut via_keepout: Vec<(f64, f64)> =
        route.vias.iter().map(|v| (v.x, v.y)).collect();
    via_keepout.extend_from_slice(banned_via_sites);
    while !remaining.is_empty() {
        let result = dijkstra_to_any(
            grid,
            &source_set,
            &remaining,
            net,
            &board.layer_stack,
            history_factor,
            present_factor,
            true, // vias allowed — recovery is exactly when they earn it
            &via_keepout,
            banned_sites,
        );
        match result {
            Some((path, reached)) => {
                remaining.remove(&reached);
                for &cell in &path {
                    source_set.insert(cell);
                }
                let span_start = route.segments.len();
                let via_start = route.vias.len();
                // Real parent: the surviving span whose copper touches
                // the attachment cell — without it, later amputation of
                // that span cannot cascade into this extension and
                // strands it (extension-era track_dangling).
                let parent = path.first().and_then(|c0| {
                    let (ax, ay) = grid.cell_center(*c0);
                    route.path_spans.iter().position(|&(ps, pl)| {
                        route.segments[ps..ps + pl].iter().any(|seg| {
                            (seg.start.0 - ax).abs() < 1e-6 && (seg.start.1 - ay).abs() < 1e-6
                                || (seg.end.0 - ax).abs() < 1e-6
                                    && (seg.end.1 - ay).abs() < 1e-6
                        })
                    })
                });
                let (segs, vias) = path_to_segments(grid, &path, share_width);
                via_keepout.extend(vias.iter().map(|v| (v.x, v.y)));
                route.segments.extend(segs);
                route.vias.extend(vias);
                // Pad-escape stub for the newly reached pin(s) at this cell.
                for (c, (px, py), _half) in &pin_targets {
                    if *c == reached {
                        let (cx, cy) = grid.cell_center(reached);
                        if (cx - px).hypot(cy - py) > 1e-6 {
                            route.segments.push(RouteSegment {
                                layer: reached.layer,
                                start: (cx, cy),
                                end: (*px, *py),
                                width_mm: share_width,
                            });
                        }
                    }
                }
                route
                    .path_spans
                    .push((span_start, route.segments.len() - span_start));
                route.path_parents.push(parent);
                route
                    .via_spans
                    .push((via_start, route.vias.len() - via_start));
                reclaimed += 1;
            }
            None => break,
        }
    }
    if reclaimed < n_missing {
        log::debug!(
            "extend_route '{}': {}/{} sinks reclaimed ({} unreachable on the \
             surviving-copper grid)",
            net.name, reclaimed, n_missing, remaining.len()
        );
    }
    reclaimed
}


/// Count pins whose pad is geometrically touched by their net's copper
/// (width-aware, same test extension recovery uses). This is the trial-
/// selection currency: "non-empty route" counts a net with one surviving
/// branch and 19 stranded pads as routed.
pub(crate) fn count_connected_sinks(board: &Board, routes: &[Route]) -> usize {
    let comp_idx: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    let mut connected = 0usize;
    for (ni, net) in board.nets.iter().enumerate() {
        let Some(route) = routes.get(ni) else { continue };
        if route.is_empty() {
            continue;
        }
        for &(comp_id, pin_id) in &net.pins {
            let Some(&ci) = comp_idx.get(&comp_id) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pin_id) else {
                continue;
            };
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.4);
            let hit = route.segments.iter().any(|sg| {
                let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                let len2 = dx * dx + dy * dy;
                let t = if len2 <= 1e-12 {
                    0.0
                } else {
                    (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / len2)
                        .clamp(0.0, 1.0)
                };
                let (cx, cy) = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                (px - cx).hypot(py - cy) < sg.width_mm / 2.0 + half - 0.001
            }) || route
                .vias
                .iter()
                .any(|v| (v.x - px).hypot(v.y - py) < half + 0.15);
            if hit {
                connected += 1;
            }
        }
    }
    connected
}
