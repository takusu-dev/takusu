//! Graph utilities for dependency DAG analysis (#355).
//!
//! All functions take an adjacency list `adj` where `adj[u]` lists the
//! out-neighbors of `u` (i.e. an edge `u → v` exists when `v ∈ adj[u]`).
//! The direction matches `build_dep_graph` in `app.rs`: an edge `u → v`
//! means "u depends on v".

use std::collections::{HashSet, VecDeque};

use petgraph::algo::toposort;
use petgraph::algo::tred::{dag_to_toposorted_adjacency_list, dag_transitive_reduction_closure};
use petgraph::graph::{DefaultIx, DiGraph, NodeIndex};
use petgraph::visit::IntoNeighbors;

/// A redundant (transitively implied) edge and a witness path that proves it.
pub(crate) struct RedundantEdge {
    pub from: usize,
    pub to: usize,
    /// `from → … → to` witness path (`from` and `to` included, length >= 3).
    pub via: Vec<usize>,
}

/// Build a `DiGraph` from an adjacency list. Node indices are `0..n` in order.
fn build_graph(adj: &[Vec<usize>]) -> DiGraph<(), (), DefaultIx> {
    let mut g = DiGraph::with_capacity(adj.len(), adj.iter().map(|v| v.len()).sum());
    let nodes: Vec<NodeIndex<DefaultIx>> = (0..adj.len()).map(|_| g.add_node(())).collect();
    for (u, outs) in adj.iter().enumerate() {
        for &v in outs {
            g.add_edge(nodes[u], nodes[v], ());
        }
    }
    g
}

/// Return `Err(node)` if the graph has a cycle (the node participates in it).
pub(crate) fn detect_cycle(adj: &[Vec<usize>]) -> Result<(), usize> {
    let g = build_graph(adj);
    match toposort(&g, None) {
        Ok(_) => Ok(()),
        Err(cycle) => Err(cycle.node_id().index()),
    }
}

/// Kahn-equivalent topological order: each node comes before its successors.
/// Returns `Err(node)` on a cycle.
pub(crate) fn topo_sort(adj: &[Vec<usize>]) -> Result<Vec<usize>, usize> {
    let g = build_graph(adj);
    match toposort(&g, None) {
        Ok(order) => Ok(order.into_iter().map(|n| n.index()).collect()),
        Err(cycle) => Err(cycle.node_id().index()),
    }
}

/// Compute the transitive reduction of the DAG and return edges that are
/// **not** in the reduction (i.e. redundant / composite edges). Each
/// redundant edge carries a witness path obtained via BFS on the graph
/// with the direct edge removed. Returns `Err(node)` if the input has a
/// cycle.
pub(crate) fn find_redundant_edges(adj: &[Vec<usize>]) -> Result<Vec<RedundantEdge>, usize> {
    let g = build_graph(adj);
    let topo = toposort(&g, None).map_err(|c| c.node_id().index())?;
    let (res, revmap) = dag_to_toposorted_adjacency_list(&g, &topo);
    let (reduction, _closure) = dag_transitive_reduction_closure(&res);

    // Collect reduction edges in rank space.
    let mut red_edges: HashSet<(usize, usize)> = HashSet::new();
    for u_rank in 0..adj.len() {
        for v_rank in reduction.neighbors(NodeIndex::<DefaultIx>::new(u_rank)) {
            red_edges.insert((u_rank, v_rank.index()));
        }
    }

    let mut redundant = Vec::new();
    for u in 0..adj.len() {
        for &v in &adj[u] {
            let u_rank = revmap[u].index();
            let v_rank = revmap[v].index();
            if !red_edges.contains(&(u_rank, v_rank)) {
                let via = bfs_witness(adj, u, v);
                redundant.push(RedundantEdge {
                    from: u,
                    to: v,
                    via,
                });
            }
        }
    }
    Ok(redundant)
}

/// BFS from `from` to `to` skipping the direct edge `from → to`, returning
/// the shortest witness path (inclusive of both endpoints). Falls back to
/// `[from, to]` if no alternate path is found (should not happen for a
/// genuine redundant edge).
fn bfs_witness(adj: &[Vec<usize>], from: usize, to: usize) -> Vec<usize> {
    let n = adj.len();
    let mut parent = vec![None::<usize>; n];
    let mut visited = vec![false; n];
    let mut queue = VecDeque::new();
    visited[from] = true;
    queue.push_back(from);
    while let Some(u) = queue.pop_front() {
        for &v in &adj[u] {
            if visited[v] {
                continue;
            }
            // Skip the direct redundant edge.
            if u == from && v == to {
                continue;
            }
            visited[v] = true;
            parent[v] = Some(u);
            if v == to {
                let mut path = vec![to];
                let mut cur = to;
                while let Some(p) = parent[cur] {
                    path.push(p);
                    cur = p;
                }
                path.reverse();
                return path;
            }
            queue.push_back(v);
        }
    }
    vec![from, to]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// adj helper: build from `(from, to)` edge list.
    fn adj(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
        let mut a = vec![Vec::new(); n];
        for &(u, v) in edges {
            a[u].push(v);
        }
        a
    }

    #[test]
    fn diamond_has_no_redundant_edges() {
        // 1→2, 1→3, 2→4, 3→4 — no transitive shortcut.
        let a = adj(4, &[(0, 1), (0, 2), (1, 3), (2, 3)]);
        let r = find_redundant_edges(&a).unwrap();
        assert!(r.is_empty(), "diamond should have no redundant edges");
    }

    #[test]
    fn simple_composite_edge() {
        // 1→2, 2→3, 1→3 — 1→3 is redundant, via = [1,2,3].
        let a = adj(3, &[(0, 1), (1, 2), (0, 2)]);
        let r = find_redundant_edges(&a).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].from, 0);
        assert_eq!(r[0].to, 2);
        assert_eq!(r[0].via, vec![0, 1, 2]);
    }

    #[test]
    fn long_path_composite_edge() {
        // 1→2→3→4 and 1→4 — 1→4 redundant, via = [1,2,3,4].
        let a = adj(4, &[(0, 1), (1, 2), (2, 3), (0, 3)]);
        let r = find_redundant_edges(&a).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].from, 0);
        assert_eq!(r[0].to, 3);
        assert_eq!(r[0].via, vec![0, 1, 2, 3]);
    }

    #[test]
    fn multiple_redundant_edges() {
        // 1→2→3→4, 1→3, 1→4, 2→4 — redundant: 1→3, 1→4, 2→4.
        let a = adj(4, &[(0, 1), (1, 2), (2, 3), (0, 2), (0, 3), (1, 3)]);
        let r = find_redundant_edges(&a).unwrap();
        let pairs: Vec<(usize, usize)> = r.iter().map(|e| (e.from, e.to)).collect();
        assert!(pairs.contains(&(0, 2)));
        assert!(pairs.contains(&(0, 3)));
        assert!(pairs.contains(&(1, 3)));
    }

    #[test]
    fn cycle_input_returns_err() {
        // 1→2, 2→1 — cycle.
        let a = adj(2, &[(0, 1), (1, 0)]);
        assert!(detect_cycle(&a).is_err());
        assert!(topo_sort(&a).is_err());
        assert!(find_redundant_edges(&a).is_err());
    }

    #[test]
    fn witness_path_uses_real_edges() {
        // 1→2, 2→3, 1→3 — verify via edges exist in adj.
        let a = adj(3, &[(0, 1), (1, 2), (0, 2)]);
        let r = find_redundant_edges(&a).unwrap();
        let via = &r[0].via;
        assert!(via.len() >= 3);
        for w in via.windows(2) {
            let u = w[0];
            let v = w[1];
            assert!(a[u].contains(&v), "witness edge {u}→{v} not in graph");
        }
    }

    #[test]
    fn topo_sort_orders_before_successors() {
        // 1→2, 1→3, 2→4, 3→4
        let a = adj(4, &[(0, 1), (0, 2), (1, 3), (2, 3)]);
        let order = topo_sort(&a).unwrap();
        let pos: std::collections::HashMap<usize, usize> =
            order.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        assert!(pos[&0] < pos[&1]);
        assert!(pos[&0] < pos[&2]);
        assert!(pos[&1] < pos[&3]);
        assert!(pos[&2] < pos[&3]);
    }

    #[test]
    fn empty_graph_has_no_redundant_edges() {
        let a: Vec<Vec<usize>> = vec![];
        assert!(find_redundant_edges(&a).unwrap().is_empty());
        assert!(detect_cycle(&a).is_ok());
        assert!(topo_sort(&a).unwrap().is_empty());
    }

    #[test]
    fn self_loop_is_cycle() {
        let a = adj(1, &[(0, 0)]);
        assert!(detect_cycle(&a).is_err());
    }
}
