//! Result navigation and expansion are independent of virtualized drawing.
//! Paths, rather than visible indices, own focus across updates and scrolling.
use hexforge_core::template::ParsedNode;
use std::collections::{BTreeMap, BTreeSet};
pub const ROW_HEIGHT: f64 = 28.0;
pub const OVERSCAN: usize = 6;
#[derive(Debug, Clone, Copy)]
pub struct ResultViewport {
    pub scroll_top: f64,
    pub height: f64,
}
#[derive(Debug, Clone, Copy)]
pub enum ResultKey {
    Down,
    Up,
    Left,
    Right,
    Home,
    End,
    Enter,
    Space,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultAction {
    Navigate { start: u64, end: u64 },
}
pub struct ResultRow<'a> {
    pub node: &'a ParsedNode,
    pub depth: usize,
    pub parent: Option<String>,
    pub position: usize,
    pub siblings: usize,
    pub index: usize,
}
#[derive(Default)]
pub struct ResultTree {
    nodes: Vec<ParsedNode>,
    expanded: BTreeSet<String>,
    active: String,
    diagnostic: String,
}
pub fn navigable_range(node: &ParsedNode) -> Option<(u64, u64)> {
    if node.kind != "leaf" || !node.diagnostics.is_empty() {
        return None;
    }
    let start = node.offset.as_ref()?.parse::<u64>().ok()?;
    let length = node.length.as_ref()?.parse::<u64>().ok()?;
    Some((start, start.checked_add(length.checked_sub(1)?)?))
}
fn visit<'a>(nodes: &'a [ParsedNode], f: &mut impl FnMut(&'a ParsedNode)) {
    for node in nodes {
        f(node);
        visit(&node.children, f);
    }
}
impl ResultTree {
    pub fn replace(&mut self, nodes: Vec<ParsedNode>) {
        if self.nodes == nodes {
            return;
        }
        let mut known = BTreeMap::new();
        visit(&self.nodes, &mut |n| {
            known.insert(n.path.clone(), n.kind.clone());
        });
        let mut next = BTreeSet::new();
        let mut paths = BTreeSet::new();
        visit(&nodes, &mut |n| {
            paths.insert(n.path.clone());
            if matches!(n.kind.as_str(), "struct" | "array")
                && if known.get(&n.path) == Some(&n.kind) {
                    self.expanded.contains(&n.path)
                } else {
                    n.kind != "array" || n.children.len() <= 32
                }
            {
                next.insert(n.path.clone());
            }
        });
        if !paths.contains(&self.active) {
            self.active = nodes.first().map_or_else(String::new, |n| n.path.clone());
        }
        if !paths.contains(&self.diagnostic) {
            self.diagnostic.clear();
        }
        self.expanded = next;
        self.nodes = nodes;
    }
    fn node(&self, path: &str) -> Option<&ParsedNode> {
        let mut found = None;
        visit(&self.nodes, &mut |n| {
            if n.path == path {
                found = Some(n)
            }
        });
        found
    }
    pub fn active_path(&self) -> &str {
        &self.active
    }
    pub fn expanded(&self, path: &str) -> bool {
        self.expanded.contains(path)
    }
    pub fn diagnostic_node(&self) -> Option<&ParsedNode> {
        self.node(&self.diagnostic)
            .filter(|n| !n.diagnostics.is_empty())
    }
    pub fn all_visible(&self) -> Vec<ResultRow<'_>> {
        fn flatten<'a>(
            nodes: &'a [ParsedNode],
            depth: usize,
            parent: Option<&str>,
            expanded: &BTreeSet<String>,
            out: &mut Vec<ResultRow<'a>>,
        ) {
            for (i, node) in nodes.iter().enumerate() {
                out.push(ResultRow {
                    node,
                    depth,
                    parent: parent.map(str::to_owned),
                    position: i + 1,
                    siblings: nodes.len(),
                    index: out.len(),
                });
                if expanded.contains(&node.path) {
                    flatten(&node.children, depth + 1, Some(&node.path), expanded, out);
                }
            }
        }
        let mut out = vec![];
        flatten(&self.nodes, 0, None, &self.expanded, &mut out);
        out
    }
    pub fn row_count(&self) -> usize {
        self.all_visible().len()
    }
    pub fn visible_nodes(&self, v: ResultViewport) -> Vec<ResultRow<'_>> {
        let rows = self.all_visible();
        let height = if v.height.is_finite() {
            v.height.max(ROW_HEIGHT)
        } else {
            ROW_HEIGHT
        };
        let maximum = (rows.len() as f64 * ROW_HEIGHT - height).max(0.0);
        let scroll = if v.scroll_top.is_finite() {
            v.scroll_top.clamp(0.0, maximum)
        } else {
            0.0
        };
        let start = (scroll / ROW_HEIGHT).floor() as usize;
        let end = ((scroll + height) / ROW_HEIGHT).ceil() as usize;
        rows.into_iter()
            .skip(start.saturating_sub(OVERSCAN))
            .take(end + OVERSCAN - start.saturating_sub(OVERSCAN))
            .collect()
    }
    pub fn navigable_leaves(&self) -> Vec<&ParsedNode> {
        let mut out = vec![];
        visit(&self.nodes, &mut |n| {
            if navigable_range(n).is_some() {
                out.push(n)
            }
        });
        out
    }
    pub fn focus(&mut self, path: &str) {
        if self.all_visible().iter().any(|r| r.node.path == path) {
            self.active = path.into();
        }
    }
    pub fn active_index(&self) -> Option<usize> {
        self.all_visible()
            .iter()
            .position(|r| r.node.path == self.active)
    }
    pub fn activate(&mut self, path: &str) -> Option<ResultAction> {
        let n = self.node(path)?;
        let container = matches!(n.kind.as_str(), "struct" | "array");
        let diagnostic = !n.diagnostics.is_empty();
        let range = navigable_range(n);
        self.active = path.into();
        self.diagnostic = if diagnostic {
            path.into()
        } else {
            String::new()
        };
        if container {
            if !self.expanded.remove(path) {
                self.expanded.insert(path.into());
            }
            None
        } else {
            range.map(|(start, end)| ResultAction::Navigate { start, end })
        }
    }
    pub fn handle_key(&mut self, key: ResultKey) -> Option<ResultAction> {
        let rows = self.all_visible();
        if rows.is_empty() {
            return None;
        }
        let i = rows
            .iter()
            .position(|r| r.node.path == self.active)
            .unwrap_or(0);
        let row = &rows[i];
        let path = row.node.path.clone();
        let parent = row.parent.clone();
        let child = row.node.children.first().map(|c| c.path.clone());
        let container = matches!(row.node.kind.as_str(), "struct" | "array");
        let destination = match key {
            ResultKey::Down => Some(rows[(i + 1).min(rows.len() - 1)].node.path.clone()),
            ResultKey::Up => Some(rows[i.saturating_sub(1)].node.path.clone()),
            ResultKey::Home => Some(rows[0].node.path.clone()),
            ResultKey::End => Some(rows.last().unwrap().node.path.clone()),
            ResultKey::Right if container => {
                if self.expanded(&path) {
                    child
                } else {
                    self.activate(&path);
                    Some(path)
                }
            }
            ResultKey::Left => {
                if self.expanded(&path) {
                    self.activate(&path);
                    Some(path)
                } else {
                    parent
                }
            }
            ResultKey::Enter | ResultKey::Space => return self.activate(&path),
            _ => None,
        };
        if let Some(path) = destination {
            self.active = path;
        }
        None
    }
}
