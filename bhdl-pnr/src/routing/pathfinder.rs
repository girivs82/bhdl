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
    // Diff-pair partners: N shadows P — when a net's partner is already
    // routed this iteration, cells near the partner's path cost less,
    // pulling the pair together (constraint synthesis v1: attraction,
    // not geometric lockstep — the sign-off report grades the result).
    let partner: HashMap<usize, usize> = {
        // (populated below; pair adjacency applied to net_order after)
        let idx_of: HashMap<NetId, usize> =
            nets.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let mut m = HashMap::new();
        for c in &board.constraints {
            if let crate::constraint::Constraint::DiffPair { p_net, n_net, .. } = c {
                if let (Some(&pi), Some(&ni)) = (idx_of.get(p_net), idx_of.get(n_net)) {
                    m.insert(pi, ni);
                    m.insert(ni, pi);
                }
            }
        }
        m
    };
    // Pair ADJACENCY: another net routed between P and N leaves its
    // copper across the corridor N wants — reorder so each pair's
    // second member routes immediately after the first.
    if !partner.is_empty() {
        let mut reordered: Vec<usize> = Vec::with_capacity(net_order.len());
        let mut placed = vec![false; nets.len()];
        for &i in &net_order {
            if placed[i] {
                continue;
            }
            reordered.push(i);
            placed[i] = true;
            if let Some(&j) = partner.get(&i) {
                if !placed[j] {
                    reordered.push(j);
                    placed[j] = true;
                }
            }
        }
        net_order = reordered;
    }

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

            // Diff-pair attraction: cells of the routed partner's path
            // (plus a one-cell ring) get discounted.
            let attract: Option<BTreeSet<CellCoord>> = partner
                .get(&net_idx)
                .filter(|&&pi| !routes[pi].is_empty())
                .map(|&pi| build_attract_set(grid, &routes[pi]));

            // Find shortest path with congestion-aware cost
            let route = shortest_path_3d(
                grid,
                net,
                board,
                &comp_idx,
                history_factor,
                present_factor,
                allow_vias,
                attract.as_ref(),
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
    attract: Option<&BTreeSet<CellCoord>>,
) -> Route {
    if net.pins.len() < 2 {
        return Route::empty(net.id);
    }

    // Solved per-sink currents (GLACIER DC), keyed by sink cell — the
    // flow analysis tapers by REAL draws when a solve ran.
    let mut sink_current: std::collections::HashMap<CellCoord, Option<f64>> =
        std::collections::HashMap::new();
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

            let natural = grid.point_to_cell(gx, gy, layer);
            let cell = escape_cell(
                grid,
                net.id,
                natural,
                (gx, gy),
                (gx - comp.x, gy - comp.y),
            );
            Some((cell, (gx, gy), comp.solved_current_a))
        })
        .collect::<Vec<(CellCoord, (f64, f64), Option<f64>)>>()
        .into_iter()
        .map(|(c, p, cur)| {
            sink_current.insert(c, cur);
            (c, p)
        })
        .collect();
    let pin_cells: Vec<CellCoord> = pin_targets.iter().map(|(c, _)| *c).collect();

    if pin_cells.len() < 2 {
        return Route::empty(net.id);
    }

    // Topology constraint: star sources every branch from the ROOT
    // pin; daisy_chain / fly_by route hop-by-hop through a nearest-
    // neighbor pin order. Default (None / T for now) = Steiner tree.
    let topo: Option<(crate::constraint::TopoKind, usize)> = board
        .constraints
        .iter()
        .find_map(|c| match c {
            crate::constraint::Constraint::Topology { net: n, kind, root, .. }
                if *n == net.id =>
            {
                // Root = the declared PinSel when it maps to one of this
                // net's pins, else pin 0 (deterministic).
                let root_idx = root
                    .and_then(|ps| {
                        net.pins
                            .iter()
                            .position(|&(c, p)| c == ps.component && p == ps.pin)
                    })
                    .unwrap_or(0);
                Some((kind.clone(), root_idx))
            }
            _ => None,
        });

    // Multi-sink Dijkstra (Steiner tree approximation)
    let root_idx = topo.as_ref().map(|(_, r)| *r).unwrap_or(0);
    let source = pin_cells[root_idx];
    let mut remaining_sinks: BTreeSet<CellCoord> = pin_cells
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != root_idx)
        .map(|(_, c)| *c)
        .collect();
    // Daisy-chain hop order: nearest-neighbor walk over pad positions
    // starting at the root. Each hop's SOURCE is the previous pin only.
    let chain_order: Option<Vec<usize>> = match topo.as_ref().map(|(k, _)| k) {
        Some(crate::constraint::TopoKind::DaisyChain)
        | Some(crate::constraint::TopoKind::FlyBy) => {
            let mut order = vec![root_idx];
            let mut left: Vec<usize> =
                (0..pin_cells.len()).filter(|&i| i != root_idx).collect();
            while !left.is_empty() {
                let &last = order.last().unwrap();
                let (lx, ly) = pin_targets[last].1;
                let (bi, _) = left
                    .iter()
                    .enumerate()
                    .min_by(|&(_, &a), &(_, &b)| {
                        let da = {
                            let (ax, ay) = pin_targets[a].1;
                            (ax - lx).hypot(ay - ly)
                        };
                        let db = {
                            let (bx, by) = pin_targets[b].1;
                            (bx - lx).hypot(by - ly)
                        };
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap();
                order.push(left.remove(bi));
            }
            Some(order)
        }
        _ => None,
    };
    let star = matches!(
        topo.as_ref().map(|(k, _)| k),
        Some(crate::constraint::TopoKind::Star)
    );
    let mut chain_hop = 1usize; // next index into chain_order

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

    let root_only: BTreeSet<CellCoord> = std::iter::once(source).collect();
    while !remaining_sinks.is_empty() {
        // Per-topology source/sink selection:
        //  - star: every branch starts at the ROOT cell
        //  - daisy chain: hop k routes FROM pin k-1 TO pin k only
        //  - default: grow the Steiner tree
        let (iter_sources, iter_sinks): (BTreeSet<CellCoord>, BTreeSet<CellCoord>) =
            if let Some(order) = &chain_order {
                if chain_hop >= order.len() {
                    break;
                }
                let from = pin_cells[order[chain_hop - 1]];
                let to = pin_cells[order[chain_hop]];
                (
                    std::iter::once(from).collect(),
                    std::iter::once(to).collect(),
                )
            } else if star {
                (root_only.clone(), remaining_sinks.clone())
            } else {
                (source_set.clone(), remaining_sinks.clone())
            };
        // Wide traces claim a ring of cells beyond their centerline —
        // without this the negotiated router threads a 2mm power trace
        // through a min-trace-sized gap and the validator amputates it
        // late (the fat-net unc family; recovery paths already ring-
        // check).
        let pitch = if grid.x_coords.len() > 1 {
            grid.x_coords[1] - grid.x_coords[0]
        } else {
            0.25
        };
        let extra_half =
            (net.required_trace_width_mm - board.config.min_trace_width_mm).max(0.0) / 2.0;
        // Hybrid rounding, measured per regime: heavy power traces
        // (extra ≥ 0.5mm each side) genuinely need the full ceil ring —
        // the partial cell holds real copper (buck's 2mm VOUT shipped
        // only with it). Mid-width nets suffer from ceil's overclaim
        // (intent_system_demo lost 2 legally-routable sinks) — floor,
        // and the validator judges the final partial cell.
        let ratio = extra_half / pitch;
        let clear_ring = if extra_half >= 0.5 {
            ratio.ceil() as usize
        } else {
            ratio.floor() as usize
        };
        let result = dijkstra_to_any(
            grid,
            &iter_sources,
            &iter_sinks,
            net,
            &board.layer_stack,
            history_factor,
            present_factor,
            allow_vias,
            &via_keepout,
            &[],
            clear_ring,
            attract,
        );

        match result {
            Some((path, reached_sink)) => {
                remaining_sinks.remove(&reached_sink);
                if chain_order.is_some() {
                    chain_hop += 1;
                }
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
    // Per-sink draw: GLACIER-solved branch current when the DC solve
    // ran for that sink's instance, else the equal-share fallback.
    // When NO sink has a solved current, keep the original integer
    // count × share math verbatim (bit-identical to the old code).
    let fallback_share = if total_sinks > 0 {
        crate::stackup::current_for_trace_width(net.required_trace_width_mm)
            / total_sinks as f64
    } else {
        0.0
    };
    let sink_draw = |p: usize| -> f64 {
        path_recs[p]
            .cells
            .last()
            .and_then(|c| sink_current.get(c).copied().flatten())
            .unwrap_or(fallback_share)
    };
    let any_solved = (0..n_paths).any(|p| {
        path_recs[p]
            .cells
            .last()
            .and_then(|c| sink_current.get(c).copied().flatten())
            .is_some()
    });
    let mut downstream_a: Vec<Vec<f64>> = Vec::new();
    let mut downstream: Vec<Vec<usize>> = path_recs
        .iter()
        .map(|r| vec![1usize; r.cells.len().max(1)])
        .collect();
    if any_solved {
        log::info!(
            "flow analysis: solved per-sink currents on '{}' ({} sink(s))",
            net.name,
            total_sinks
        );
        downstream_a = path_recs
            .iter()
            .enumerate()
            .map(|(p, r)| vec![sink_draw(p); r.cells.len().max(1)])
            .collect();
        for p in (0..n_paths).rev() {
            let served: f64 = *downstream_a[p].first().unwrap_or(&0.0);
            if let Some((pp, pi)) = path_recs[p].parent {
                for i in 0..=pi.min(downstream_a[pp].len().saturating_sub(1)) {
                    downstream_a[pp][i] += served;
                }
            }
        }
    } else {
        // Children contribute to the parent prefix [0..=attach_idx].
        // Children always have HIGHER indices than their parents, so
        // one reverse pass suffices.
        for p in (0..n_paths).rev() {
            let served: usize = *downstream[p].first().unwrap_or(&1);
            if let Some((pp, pi)) = path_recs[p].parent {
                for i in 0..=pi.min(downstream[pp].len().saturating_sub(1)) {
                    downstream[pp][i] += served;
                }
            }
        }
    }
    let share = fallback_share;
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
            let seg_current = if any_solved {
                downstream_a[p][w + 1]
            } else {
                share * downstream[p][w + 1] as f64
            };
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

/// ESCAPE STUB target selection: a fine-pitch pin's natural cell can
/// be walled in — every planar neighbor inside some OTHER pad's halo —
/// even though a clearance-true escape lane exists straight out along
/// the pad axis (cell-granularity halo painting covers the lane's cell
/// centers). When the natural cell has no passable approach, walk
/// OUTWARD (component center → pad direction) to the first cell that
/// is passable AND has a passable neighbor, and target THAT cell; the
/// existing stub emission draws the pad→waypoint copper and the
/// geometric validator polices it like any other segment (an illegal
/// escape is amputated to honest unconnected).
fn escape_cell(
    grid: &RoutingGrid,
    net_id: NetId,
    natural: CellCoord,
    pad_xy: (f64, f64),
    outward: (f64, f64),
) -> CellCoord {
    let passable = |c: CellCoord| -> bool {
        let g = grid.get(c);
        !(g.hard || g.nc_blocked || (g.blocked && !g.owners.contains(&net_id)))
    };
    let has_open_neighbor = |c: CellCoord| -> bool {
        grid.planar_neighbors(c).into_iter().any(|(n, _)| passable(n))
    };
    // Natural cell approachable? Keep it.
    if has_open_neighbor(natural) {
        return natural;
    }
    let len = (outward.0 * outward.0 + outward.1 * outward.1).sqrt();
    if len < 1e-9 {
        return natural;
    }
    let (ux, uy) = (outward.0 / len, outward.1 / len);
    let mut t = 0.3f64;
    while t <= 3.0 {
        let c = grid.point_to_cell(pad_xy.0 + ux * t, pad_xy.1 + uy * t, natural.layer);
        // The stub is drawn pad -> CELL CENTER: a center laterally off
        // the escape axis makes a diagonal stub that clips the very
        // neighbor pads we are escaping (validator amputates it, net
        // loses). Only accept cells whose center sits ON the axis.
        let (ccx, ccy) = grid.cell_center(c);
        let lateral = ((ccx - pad_xy.0) * uy - (ccy - pad_xy.1) * ux).abs();
        if lateral > 0.08 {
            t += 0.3;
            continue;
        }
        if c != natural && passable(c) && has_open_neighbor(c) {
            log::debug!(
                "escape stub: pin at ({:.2},{:.2}) walled in — waypoint {:.1}mm outward",
                pad_xy.0, pad_xy.1, t
            );
            return c;
        }
        t += 0.3;
    }
    natural
}

/// Dijkstra from any source cell to any sink cell.
#[allow(clippy::too_many_arguments)]
/// The attraction corridor of a routed diff-pair partner: its cells,
/// a one-cell ring (the target gap at grid pitch), projected to every
/// layer so a shadow net forced through a via still follows the XY run.
pub(crate) fn build_attract_set(grid: &RoutingGrid, route: &Route) -> BTreeSet<CellCoord> {
    let mut cells = route_cells(grid, route);
    // TWO rings: ring 1 is the target gap at grid pitch, but in
    // recovery grids the partner's copper is hard-blocked INCLUDING
    // its spacing halo — ring 1 is unreachable there and ring 2 is the
    // first legal offset. The band covers both.
    for _ in 0..2 {
        let ring: Vec<CellCoord> = cells
            .iter()
            .flat_map(|c| grid.planar_neighbors(*c).into_iter().map(|(n, _)| n))
            .collect();
        cells.extend(ring);
    }
    let projected: Vec<CellCoord> = cells
        .iter()
        .flat_map(|c| {
            (0..grid.num_layers).map(|l| CellCoord {
                col: c.col,
                row: c.row,
                layer: l,
            })
        })
        .collect();
    cells.extend(projected);
    cells
}

#[allow(clippy::too_many_arguments)]
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
    clear_ring: usize,
    attract: Option<&BTreeSet<CellCoord>>,
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
            // Wide-trace clearance: grid halos are sized for the
            // MINIMUM trace width; a trunk routed wider under-clears
            // foreign pads by construction (the validator then rips the
            // root span and the loop ping-pongs). Require a clear
            // Chebyshev ring proportional to the extra half-width.
            if clear_ring > 0 && !sinks.contains(&nbr) {
                let mut ok = true;
                'ring: for dr in -(clear_ring as i64)..=(clear_ring as i64) {
                    for dc in -(clear_ring as i64)..=(clear_ring as i64) {
                        if dr == 0 && dc == 0 {
                            continue;
                        }
                        let rr = nbr.row as i64 + dr;
                        let cc = nbr.col as i64 + dc;
                        if rr < 0
                            || cc < 0
                            || rr as usize >= grid.rows()
                            || cc as usize >= grid.cols()
                        {
                            ok = false; // wide copper would leave the board
                            break 'ring;
                        }
                        let rc = CellCoord {
                            row: rr as usize,
                            col: cc as usize,
                            ..nbr
                        };
                        let g = grid.get(rc);
                        if g.hard
                            || g.nc_blocked
                            || (g.blocked && !g.owners.contains(&net.id))
                        {
                            ok = false;
                            break 'ring;
                        }
                    }
                }
                if !ok {
                    continue;
                }
            }
            // Layer rule: the net's copper may only exist on its
            // allowed layers.
            if let Some(allowed) = &net.allowed_layers {
                if !allowed.contains(&nbr.layer) && !sinks.contains(&nbr) {
                    continue;
                }
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
            // Diff-pair attraction: riding alongside the routed partner
            // is cheap — the pair converges to a coupled run without
            // geometric lockstep.
            let attract_factor = match attract {
                Some(set) if set.contains(&nbr) => 0.15,
                _ => 1.0,
            };
            let new_cost = cost + edge_cost * width_factor * move_cost * attract_factor;

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

                if let Some(allowed) = &net.allowed_layers {
                    if !allowed.contains(&nbr.layer) {
                        continue;
                    }
                }
                // A diff-pair shadow net leaving its partner's layer
                // abandons the coupled run entirely — vias cost 4× for
                // it (an EMPTY far layer otherwise wins on congestion).
                let via_factor = if attract.is_some() { 4.0 } else { 1.0 };
                let new_cost = cost + grid.via_cost * via_factor;
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
/// Routed plane-drop fallback: when no straight stub from the pad
/// reaches a legal via site (post-recovery congestion), route one with
/// dijkstra on the pad's layer — sinks are every grid cell whose center
/// passes the caller's `site_ok`. Returns the stub segments (pad-escape
/// included) and the via position, or None if the pad is truly walled
/// in (honest unconnected).
pub(crate) fn routed_plane_drop(
    grid: &RoutingGrid,
    net: &PnrNet,
    board: &Board,
    pad: (f64, f64),
    comp_center: (f64, f64),
    stub_layer: usize,
    share: f64,
    site_ok: &dyn Fn(f64, f64) -> bool,
) -> Option<(Vec<RouteSegment>, (f64, f64))> {
    let natural = grid.point_to_cell(pad.0, pad.1, stub_layer);
    let start = escape_cell(
        grid,
        net.id,
        natural,
        pad,
        (pad.0 - comp_center.0, pad.1 - comp_center.1),
    );
    let mut sources = BTreeSet::new();
    sources.insert(start);
    let mut sinks: BTreeSet<CellCoord> = BTreeSet::new();
    for (col, &x) in grid.x_coords.iter().enumerate() {
        if (x - pad.0).abs() > 20.0 {
            continue;
        }
        for (row, &y) in grid.y_coords.iter().enumerate() {
            if (y - pad.1).abs() > 20.0 {
                continue;
            }
            if site_ok(x, y) {
                sinks.insert(CellCoord { col, row, layer: stub_layer });
            }
        }
    }
    if sinks.is_empty() {
        return None;
    }
    let pitch = if grid.x_coords.len() > 1 {
        grid.x_coords[1] - grid.x_coords[0]
    } else {
        0.25
    };
    let clear_ring = (((share - board.config.min_trace_width_mm).max(0.0) / 2.0)
        / pitch)
        .ceil() as usize;
    let (path, reached) = dijkstra_to_any(
        grid,
        &sources,
        &sinks,
        net,
        &board.layer_stack,
        1.0,
        1.0,
        false, // stub stays on the pad's layer; the via IS the drop
        &[],
        &[],
        clear_ring,
        None,
    )?;
    let (mut segs, _vias) = path_to_segments(grid, &path, share);
    let (sx, sy) = grid.cell_center(start);
    if (sx - pad.0).hypot(sy - pad.1) > 1e-6 {
        segs.insert(
            0,
            RouteSegment {
                layer: stub_layer,
                start: pad,
                end: (sx, sy),
                width_mm: share,
            },
        );
    }
    Some((segs, grid.cell_center(reached)))
}

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
/// Route ONE net on the given grid — the full topology-aware
/// construction (star roots, daisy hops, Steiner default). Recovery
/// uses this to REBUILD a topology-constrained net wholesale:
/// extension/greedy tree repair would regrow it as a Steiner tree and
/// silently lose the declared shape.
pub(crate) fn route_single_net(
    grid: &RoutingGrid,
    net: &PnrNet,
    board: &Board,
    allow_vias: bool,
    attract: Option<&BTreeSet<CellCoord>>,
) -> Route {
    let comp_idx: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    shortest_path_3d(grid, net, board, &comp_idx, 1.0, 1.0, allow_vias, attract)
}

/// Grid-free count of a net's pins whose pad is NOT touched by
/// tree-connected copper — the cheap pre-filter for the completion
/// pass (building an extension grid per net is the expensive part).
pub(crate) fn unreached_sink_count(net: &PnrNet, board: &Board, route: &Route) -> usize {
    if route.is_empty() {
        return net.pins.len();
    }
    let comp_idx: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    let comps = route_components(route);
    let tree_comp: Option<usize> = {
        let mut pop: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        for &c in &comps {
            *pop.entry(c).or_insert(0) += 1;
        }
        pop.into_iter().max_by_key(|&(c, n)| (n, std::cmp::Reverse(c))).map(|(c, _)| c)
    };
    let mut unreached = 0usize;
    for &(comp_id, pin_id) in &net.pins {
        let Some(&ci) = comp_idx.get(&comp_id) else { continue };
        let comp = &board.components[ci];
        let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pin_id) else { continue };
        if pin.unplaced {
            continue;
        }
        let cos_t = comp.theta.cos();
        let sin_t = comp.theta.sin();
        let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
        let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
        let half = pin
            .pad
            .as_ref()
            .map(|p| p.width_mm.min(p.height_mm) / 2.0)
            .unwrap_or(0.4);
        let touched = route.segments.iter().enumerate().any(|(si, sg)| {
            if Some(comps[si]) != tree_comp {
                return false;
            }
            let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
            let len2 = dx * dx + dy * dy;
            let t = if len2 <= 1e-12 {
                0.0
            } else {
                (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / len2).clamp(0.0, 1.0)
            };
            let (cx, cy) = (sg.start.0 + t * dx, sg.start.1 + t * dy);
            (px - cx).hypot(py - cy) < sg.width_mm / 2.0 + half - 0.001
        });
        if !touched {
            unreached += 1;
        }
    }
    unreached
}

#[allow(clippy::too_many_arguments)]
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
    attract: Option<&BTreeSet<CellCoord>>,
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
            let natural = grid.point_to_cell(gx, gy, layer);
            let cell = escape_cell(
                grid,
                net.id,
                natural,
                (gx, gy),
                (gx - comp.x, gy - comp.y),
            );
            Some((cell, (gx, gy), half))
        })
        .collect();

    // Tree-connected pads only: an orphan fragment touching the pad is
    // NOT a connection (it made extension skip pins whose copper was a
    // stranded stub — shipped as unconnected). The tree = the LARGEST
    // copper component of the surviving route.
    let comps = route_components(route);
    let tree_comp: Option<usize> = {
        let mut pop: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        for &c in &comps {
            *pop.entry(c).or_insert(0) += 1;
        }
        pop.into_iter().max_by_key(|&(c, n)| (n, std::cmp::Reverse(c))).map(|(c, _)| c)
    };
    // Existing TREE copper = source set; unreached pins = sinks.
    // Sources must come from the tree component only: seeding from ALL
    // copper let recovery attach a new path to an orphan fragment (an
    // amputation leftover) — "reached" per the model, an island per
    // KiCad. "Reached" is GEOMETRY, not cell membership: route_cells
    // includes a fat trunk's width ring, and a pin cell covered by the
    // ring can still have no copper touching its pad (that unc was
    // invisible here — extension thought there was nothing to do).
    let tree_route = Route {
        segments: route
            .segments
            .iter()
            .enumerate()
            .filter(|(si, _)| Some(comps[*si]) == tree_comp)
            .map(|(_, sg)| sg.clone())
            .collect(),
        vias: route
            .vias
            .iter()
            .filter(|v| {
                route.segments.iter().enumerate().any(|(si, sg)| {
                    Some(comps[si]) == tree_comp
                        && ((v.x - sg.start.0).hypot(v.y - sg.start.1) < 0.02
                            || (v.x - sg.end.0).hypot(v.y - sg.end.1) < 0.02)
                })
            })
            .cloned()
            .collect(),
        ..Route::empty(route.net_id)
    };
    let mut source_set: BTreeSet<CellCoord> = route_cells(grid, &tree_route);
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
        route.segments.iter().enumerate().any(|(si, sg)| {
            if Some(comps[si]) != tree_comp {
                return false;
            }
            let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
            let len2 = dx * dx + dy * dy;
            let t = if len2 <= 1e-12 {
                0.0
            } else {
                (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / len2).clamp(0.0, 1.0)
            };
            let (cx, cy) = (sg.start.0 + t * dx, sg.start.1 + t * dy);
            (px - cx).hypot(py - cy) < sg.width_mm / 2.0 + half - 0.001
        })
    };
    let mut remaining: BTreeSet<CellCoord> = pin_targets
        .iter()
        .filter(|(_, (px, py), half)| !pad_connected(*px, *py, *half))
        .map(|(c, _, _)| *c)
        .collect();
    if let Ok(filt) = std::env::var("BHDL_PNR_DEBUG_NETS") {
        if filt != "1" && net.name.contains(&filt) {
            for (c, (px, py), half) in &pin_targets {
                let nearest = route
                    .segments
                    .iter()
                    .map(|sg| {
                        let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                        let len2 = dx * dx + dy * dy;
                        let t = if len2 <= 1e-12 {
                            0.0
                        } else {
                            (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / len2)
                                .clamp(0.0, 1.0)
                        };
                        let (cx, cy) = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                        (px - cx).hypot(py - cy) - sg.width_mm / 2.0
                    })
                    .fold(f64::INFINITY, f64::min);
                log::warn!(
                    "extend_route '{}' pin ({px:.2},{py:.2}) half={half:.2}:                      connected={} nearest_copper_edge={nearest:.3} in_remaining={}                      segs={} comps_tree={:?}",
                    net.name,
                    pad_connected(*px, *py, *half),
                    remaining.contains(c),
                    route.segments.len(),
                    tree_comp
                );
            }
        }
    }
    let share_width = if full_width {
        // From-scratch trunks may end up carrying the whole rail; use
        // the IPC width for the full current rather than a leaf share.
        net.required_trace_width_mm
    } else if !matches!(
        net.net_class,
        crate::types::PnrNetClass::Power { .. } | crate::types::PnrNetClass::Ground
    ) {
        // Leaf-share TAPER is a power-tree concept (current splits
        // across sinks). A SIGNAL net's width is a rule — the
        // impedance floor in particular must hold on every segment
        // (the oracle showed a floored net extended at 0.15mm under a
        // 0.17mm floor).
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

    // Extra clearance cells for traces wider than the pitch's assumed
    // minimum: pitch = min_trace + min_spacing, halos cover min_trace/2.
    let pitch = if grid.x_coords.len() > 1 {
        grid.x_coords[1] - grid.x_coords[0]
    } else {
        0.25
    };
    let extra_half = (share_width - board.config.min_trace_width_mm).max(0.0) / 2.0;
    let clear_ring = (extra_half / pitch).ceil() as usize;
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
            clear_ring,
            attract,
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
                    let endpoint_hit = route.path_spans.iter().position(|&(ps, pl)| {
                        route.segments[ps..ps + pl].iter().any(|seg| {
                            (seg.start.0 - ax).abs() < 1e-6 && (seg.start.1 - ay).abs() < 1e-6
                                || (seg.end.0 - ax).abs() < 1e-6
                                    && (seg.end.1 - ay).abs() < 1e-6
                        })
                    });
                    // Mid-segment attach: endpoint equality misses it,
                    // parent = None, and later amputation of the host
                    // span cannot cascade here — orphan fragments.
                    endpoint_hit.or_else(|| {
                        route.path_spans.iter().position(|&(ps, pl)| {
                            route.segments[ps..ps + pl].iter().any(|seg| {
                                let (dx, dy) =
                                    (seg.end.0 - seg.start.0, seg.end.1 - seg.start.1);
                                let l2 = dx * dx + dy * dy;
                                let t = if l2 <= 1e-12 {
                                    0.0
                                } else {
                                    (((ax - seg.start.0) * dx + (ay - seg.start.1) * dy)
                                        / l2)
                                        .clamp(0.0, 1.0)
                                };
                                (ax - (seg.start.0 + t * dx))
                                    .hypot(ay - (seg.start.1 + t * dy))
                                    < seg.width_mm / 2.0 + 0.02
                            })
                        })
                    })
                });
                let (segs, vias) = path_to_segments(grid, &path, share_width);
                via_keepout.extend(vias.iter().map(|v| (v.x, v.y)));
                // JOINT: the attach cell can be a width-ring cell of a
                // fat trunk — covered by route_cells but with no copper
                // AT the cell center. An extension starting there is
                // endpoint-graph dangling (KiCad and the validator both
                // see a stray end) and the guarantee trim eats the
                // whole path. Weld it: a short segment from the nearest
                // point ON existing same-layer copper to the attach
                // point.
                if let Some(c0) = path.first() {
                    let (ax, ay) = grid.cell_center(*c0);
                    let anchored = route
                        .segments
                        .iter()
                        .enumerate()
                        .any(|(si, sg)| {
                            comps.get(si).map_or(true, |c| Some(*c) == tree_comp)
                                && sg.layer == c0.layer
                                && {
                                    let (dx, dy) =
                                        (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                                    let l2 = dx * dx + dy * dy;
                                    let t = if l2 <= 1e-12 {
                                        0.0
                                    } else {
                                        (((ax - sg.start.0) * dx + (ay - sg.start.1) * dy)
                                            / l2)
                                            .clamp(0.0, 1.0)
                                    };
                                    (ax - (sg.start.0 + t * dx))
                                        .hypot(ay - (sg.start.1 + t * dy))
                                        < 0.01
                                }
                        })
                        || route
                            .vias
                            .iter()
                            .any(|v| (v.x - ax).hypot(v.y - ay) < 0.01);
                    if !anchored {
                        let mut best: Option<((f64, f64), f64)> = None;
                        for (si, sg) in route.segments.iter().enumerate() {
                            if !(comps.get(si).map_or(true, |c| Some(*c) == tree_comp))
                                || sg.layer != c0.layer
                            {
                                continue;
                            }
                            let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                            let l2 = dx * dx + dy * dy;
                            let t = if l2 <= 1e-12 {
                                0.0
                            } else {
                                (((ax - sg.start.0) * dx + (ay - sg.start.1) * dy) / l2)
                                    .clamp(0.0, 1.0)
                            };
                            let (jx, jy) = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                            let d = (ax - jx).hypot(ay - jy);
                            if best.as_ref().map_or(true, |&(_, bd)| d < bd) {
                                best = Some(((jx, jy), d));
                            }
                        }
                        if let Some(((jx, jy), d)) = best {
                            if d > 1e-6 && d < 1.5 {
                                route.segments.push(RouteSegment {
                                    layer: c0.layer,
                                    start: (jx, jy),
                                    end: (ax, ay),
                                    width_mm: share_width,
                                });
                            }
                        }
                    }
                }
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
        if std::env::var("BHDL_PNR_DEBUG_NETS").is_ok() {
            for cell in &remaining {
                let (cx, cy) = grid.cell_center(*cell);
                let gc = grid.get(*cell);
                log::warn!(
                    "extend_route '{}' STUCK sink cell ({cx:.2},{cy:.2}) L{}: \
                     blocked={} hard={} nc={} owners_has_net={} demand={}/{} \
                     src_cells={} banned_sites={}",
                    net.name, cell.layer, gc.blocked, gc.hard, gc.nc_blocked,
                    gc.owners.contains(&net.id), gc.demand, gc.capacity,
                    source_set.len(), banned_sites.len()
                );
                // Neighbor ring: is the sink an island?
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let nx = cell.col as i32 + dx;
                    let ny = cell.row as i32 + dy;
                    if nx < 0 || ny < 0 {
                        continue;
                    }
                    let nc = CellCoord { col: nx as usize, row: ny as usize, layer: cell.layer };
                    if nc.col >= grid.x_coords.len() || nc.row >= grid.y_coords.len() {
                        continue;
                    }
                    let g = grid.get(nc);
                    log::warn!(
                        "    nbr({dx:+},{dy:+}): blocked={} hard={} nc={} owners_has_net={}",
                        g.blocked, g.hard, g.nc_blocked, g.owners.contains(&net.id)
                    );
                }
            }
        }
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
        if route.is_empty() && net.plane_layer.is_none() {
            continue;
        }
        // Ratsnest semantics: a pin is connected only if its touching
        // copper COMPONENT also reaches another pin (an orphan stub
        // touching one pad is not a connection).
        let comps = route_components(route);
        let mut pin_comp: Vec<Option<usize>> = Vec::new();
        let mut pin_thru_plane: Vec<bool> = Vec::new();
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
            // Plane-net through-hole barrels pierce their plane fill.
            if net.plane_layer.is_some()
                && pin.pad.as_ref().and_then(|p| p.drill_mm).is_some()
            {
                pin_thru_plane.push(true);
                pin_comp.push(None);
                continue;
            }
            pin_thru_plane.push(false);
            let hit = route.segments.iter().enumerate().find_map(|(si, sg)| {
                let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                let len2 = dx * dx + dy * dy;
                let t = if len2 <= 1e-12 {
                    0.0
                } else {
                    (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / len2)
                        .clamp(0.0, 1.0)
                };
                let (cx, cy) = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                ((px - cx).hypot(py - cy) < sg.width_mm / 2.0 + half - 0.001)
                    .then_some(comps[si])
            });
            pin_comp.push(hit);
        }
        // Component pin-population: a plane net's plane counts as a
        // "pin" every drop-via component reaches (via drops connect
        // stub components through the fill).
        let mut pop: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        for c in pin_comp.iter().flatten() {
            *pop.entry(*c).or_insert(0) += 1;
        }
        let plane_bonus = net.plane_layer.is_some();
        for (k, &c) in pin_comp.iter().enumerate() {
            if pin_thru_plane[k] {
                connected += 1;
                continue;
            }
            match c {
                Some(cc) if pop.get(&cc).copied().unwrap_or(0) >= 2 || plane_bonus => {
                    connected += 1;
                }
                _ => {}
            }
        }
    }
    connected
}


/// Hard-block a surviving route's copper GEOMETRY on a recovery grid:
/// every cell whose center is within (seg_width/2 + spacing +
/// min_trace/2) of a segment, plus via barrels. Cell-set blocking
/// (route_cells) under-covers a wide trunk by up to half a pitch — a
/// recovery route through the adjacent cell under-cleared it and the
/// validator ripped the rebuild every round.
pub(crate) fn block_route_geometry(
    grid: &mut RoutingGrid,
    route: &Route,
    board: &Board,
) {
    let pitch = if grid.x_coords.len() > 1 {
        grid.x_coords[1] - grid.x_coords[0]
    } else {
        0.25
    };
    let spacing = board.config.min_spacing_mm;
    let min_half = board.config.min_trace_width_mm / 2.0;
    for seg in &route.segments {
        let margin = seg.width_mm / 2.0 + spacing + min_half;
        let x_lo = seg.start.0.min(seg.end.0) - margin;
        let x_hi = seg.start.0.max(seg.end.0) + margin;
        let y_lo = seg.start.1.min(seg.end.1) - margin;
        let y_hi = seg.start.1.max(seg.end.1) + margin;
        let c_lo = grid.point_to_cell(x_lo, y_lo, seg.layer);
        let c_hi = grid.point_to_cell(x_hi, y_hi, seg.layer);
        for row in c_lo.row..=c_hi.row {
            for col in c_lo.col..=c_hi.col {
                let cc = CellCoord { layer: seg.layer, row, col };
                let (cx, cy) = grid.cell_center(cc);
                // point-to-segment distance
                let (dx, dy) = (seg.end.0 - seg.start.0, seg.end.1 - seg.start.1);
                let len2 = dx * dx + dy * dy;
                let t = if len2 <= 1e-12 {
                    0.0
                } else {
                    (((cx - seg.start.0) * dx + (cy - seg.start.1) * dy) / len2)
                        .clamp(0.0, 1.0)
                };
                let (px, py) = (seg.start.0 + t * dx, seg.start.1 + t * dy);
                if (cx - px).hypot(cy - py) < margin + pitch * 0.5 {
                    let c = grid.get_mut(cc);
                    c.blocked = true;
                    c.hard = true;
                }
            }
        }
    }
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    for v in &route.vias {
        let margin = via_r + spacing + min_half;
        for layer in 0..grid.num_layers {
            let c_lo = grid.point_to_cell(v.x - margin, v.y - margin, layer);
            let c_hi = grid.point_to_cell(v.x + margin, v.y + margin, layer);
            for row in c_lo.row..=c_hi.row {
                for col in c_lo.col..=c_hi.col {
                    let cc = CellCoord { layer, row, col };
                    let (cx, cy) = grid.cell_center(cc);
                    if (cx - v.x).hypot(cy - v.y) < margin + pitch * 0.5 {
                        let c = grid.get_mut(cc);
                        c.blocked = true;
                        c.hard = true;
                    }
                }
            }
        }
    }
}


/// Connected components of a route's copper: segments union by shared
/// endpoints (or endpoint-on-interior T-joints), vias union everything
/// at their position. Returns per-segment component ids. The
/// "pad-touched = connected" shortcut counted ORPHAN fragments (a
/// leftover stub touching the pad) as reached pins — ratsnest truth
/// requires the touching copper to reach ANOTHER pin of the net.
pub(crate) fn route_components(route: &Route) -> Vec<usize> {
    let n = route.segments.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut Vec<usize>, i: usize) -> usize {
        if p[i] != i {
            let r = find(p, p[i]);
            p[i] = r;
        }
        p[i]
    }
    let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) < 0.02;
    let on_seg = |sg: &RouteSegment, pt: (f64, f64)| -> bool {
        let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
        let l2 = dx * dx + dy * dy;
        let t = if l2 <= 1e-12 {
            0.0
        } else {
            (((pt.0 - sg.start.0) * dx + (pt.1 - sg.start.1) * dy) / l2).clamp(0.0, 1.0)
        };
        (pt.0 - (sg.start.0 + t * dx)).hypot(pt.1 - (sg.start.1 + t * dy)) < 0.02
    };
    for i in 0..n {
        for j in (i + 1)..n {
            let (a, b) = (&route.segments[i], &route.segments[j]);
            if a.layer == b.layer
                && (near(a.start, b.start)
                    || near(a.start, b.end)
                    || near(a.end, b.start)
                    || near(a.end, b.end)
                    || on_seg(b, a.start)
                    || on_seg(b, a.end)
                    || on_seg(a, b.start)
                    || on_seg(a, b.end))
            {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }
    // Vias join segments across layers at their position.
    for v in &route.vias {
        let mut first: Option<usize> = None;
        for i in 0..n {
            if on_seg(&route.segments[i], (v.x, v.y)) {
                match first {
                    None => first = Some(i),
                    Some(f) => {
                        let (ri, rf) = (find(&mut parent, i), find(&mut parent, f));
                        if ri != rf {
                            parent[ri] = rf;
                        }
                    }
                }
            }
        }
    }
    (0..n).map(|i| find(&mut parent, i)).collect()
}
