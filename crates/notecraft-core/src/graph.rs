//! The graph of `[[links]]` between documents, and a layout for drawing it.

use crate::blocks::Block;
use crate::workspace::{DocId, Workspace};
use std::collections::{BTreeMap, BTreeSet};

/// Documents that link to or are linked from another one, and the links between them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkGraph {
    pub nodes: Vec<DocId>,
    pub titles: Vec<String>,
    /// Pairs of node indexes (a link in either direction counts once).
    pub edges: Vec<(usize, usize)>,
    /// How many links touch each node.
    pub degree: Vec<usize>,
}

/// More nodes than this are drawn only for the best connected documents.
pub const MAX_NODES: usize = 600;

fn mentions(blocks: &[Block]) -> Vec<String> {
    let mut v = Vec::new();
    for b in blocks {
        if b.kind.has_rich_text() {
            for m in crate::inline::mentions(&b.text) {
                if !v.contains(&m) {
                    v.push(m);
                }
            }
        }
    }
    v
}

impl Workspace {
    /// Every `[[link]]` between pages, databases and rows that exist (and are not in the trash).
    pub fn link_graph(&self) -> LinkGraph {
        let mut pairs: BTreeSet<(DocId, DocId)> = BTreeSet::new();
        let mut add = |from: DocId, blocks: &[Block]| {
            if !self.is_live(&from) {
                return;
            }
            for title in mentions(blocks) {
                if let Some(to) = self.resolve_title(&title) {
                    if to != from {
                        let (a, b) = if from < to { (from.clone(), to) } else { (to, from.clone()) };
                        pairs.insert((a, b));
                    }
                }
            }
        };
        for (id, p) in &self.pages {
            if !p.is_locked() {
                add(DocId::Page(id.clone()), &p.blocks);
            }
        }
        for (db, d) in &self.dbs {
            for (rid, r) in &d.rows {
                add(DocId::Row(db.clone(), rid.clone()), &r.blocks);
            }
        }
        let mut degree: BTreeMap<DocId, usize> = BTreeMap::new();
        for (a, b) in &pairs {
            *degree.entry(a.clone()).or_default() += 1;
            *degree.entry(b.clone()).or_default() += 1;
        }
        let mut ranked: Vec<(DocId, usize)> = degree.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        ranked.truncate(MAX_NODES);
        let index: BTreeMap<DocId, usize> = ranked.iter().enumerate().map(|(i, (d, _))| (d.clone(), i)).collect();
        let mut g = LinkGraph {
            nodes: ranked.iter().map(|(d, _)| d.clone()).collect(),
            titles: ranked.iter().map(|(d, _)| self.title(d)).collect(),
            edges: Vec::new(),
            degree: vec![0; ranked.len()],
        };
        for (a, b) in &pairs {
            if let (Some(&i), Some(&j)) = (index.get(a), index.get(b)) {
                g.edges.push((i, j));
                g.degree[i] += 1;
                g.degree[j] += 1;
            }
        }
        g
    }
}

/// Positions for `n` nodes in roughly the unit square, with linked nodes close and the others
/// apart (Fruchterman-Reingold). Deterministic: the same graph always gives the same picture.
pub fn layout(n: usize, edges: &[(usize, usize)], iterations: usize) -> Vec<(f32, f32)> {
    if n == 0 {
        return Vec::new();
    }
    let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    let mut pos: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let r = 0.5 * ((i as f32 + 0.5) / n as f32).sqrt();
            let a = i as f32 * golden;
            (0.5 + r * a.cos(), 0.5 + r * a.sin())
        })
        .collect();
    if n == 1 {
        return pos;
    }
    let k = (1.0 / n as f32).sqrt();
    let mut temp = 0.1_f32;
    for _ in 0..iterations {
        let mut disp = vec![(0.0_f32, 0.0_f32); n];
        for i in 0..n {
            for j in (i + 1)..n {
                let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                let d2 = (dx * dx + dy * dy).max(1e-6);
                let d = d2.sqrt();
                let f = k * k / d;
                let (fx, fy) = (dx / d * f, dy / d * f);
                disp[i].0 += fx;
                disp[i].1 += fy;
                disp[j].0 -= fx;
                disp[j].1 -= fy;
            }
        }
        for &(a, b) in edges {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let d = (dx * dx + dy * dy).sqrt().max(1e-6);
            let f = d * d / k;
            let (fx, fy) = (dx / d * f, dy / d * f);
            disp[a].0 -= fx;
            disp[a].1 -= fy;
            disp[b].0 += fx;
            disp[b].1 += fy;
        }
        for i in 0..n {
            // a light pull to the centre keeps separate islands on the page
            disp[i].0 -= (pos[i].0 - 0.5) * 0.4;
            disp[i].1 -= (pos[i].1 - 0.5) * 0.4;
            let d = (disp[i].0 * disp[i].0 + disp[i].1 * disp[i].1).sqrt().max(1e-6);
            let step = d.min(temp);
            pos[i].0 += disp[i].0 / d * step;
            pos[i].1 += disp[i].1 / d * step;
        }
        temp *= 0.97;
    }
    // fit into [0, 1] on both axes
    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in &pos {
        x0 = x0.min(p.0);
        x1 = x1.max(p.0);
        y0 = y0.min(p.1);
        y1 = y1.max(p.1);
    }
    let (w, h) = ((x1 - x0).max(1e-6), (y1 - y0).max(1e-6));
    pos.iter().map(|p| ((p.0 - x0) / w, (p.1 - y0) / h)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_nodes_end_up_closer_than_unlinked_ones() {
        // 0-1-2 chain and a separate pair 3-4
        let edges = [(0, 1), (1, 2), (3, 4)];
        let pos = layout(5, &edges, 200);
        let d = |a: usize, b: usize| ((pos[a].0 - pos[b].0).powi(2) + (pos[a].1 - pos[b].1).powi(2)).sqrt();
        assert!(d(0, 1) < d(0, 3));
        assert!(d(3, 4) < d(2, 3));
        assert!(pos.iter().all(|p| (0.0..=1.0).contains(&p.0) && (0.0..=1.0).contains(&p.1)));
        // same input, same picture
        assert_eq!(pos, layout(5, &edges, 200));
        assert!(layout(0, &[], 10).is_empty());
        assert_eq!(layout(1, &[], 10).len(), 1);
    }
}
