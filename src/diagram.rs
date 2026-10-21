//! Host topology arrangement. Node content sizes come only from prepared displays.
use anyhow::{Result, ensure};
use eframe::egui::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug)]
pub struct Item {
    pub key: String,
    pub parent: Option<String>,
    pub size: Vec2,
}
/// Rightward layered tidy arrangement; never measures or wraps Goo content.
pub fn arrange(items: &[Item]) -> Result<BTreeMap<String, Rect>> {
    ensure!(items.len() <= 4096, "Diagram exceeds materialization bound");
    if items.is_empty() {
        return Ok(BTreeMap::new());
    }
    let by: BTreeMap<_, _> = items.iter().map(|n| (n.key.clone(), n)).collect();
    ensure!(by.len() == items.len(), "Duplicate diagram identity");
    ensure!(
        items.iter().filter(|n| n.parent.is_none()).count() == 1,
        "Expected one diagram root"
    );
    let mut depth = BTreeMap::new();
    let mut widths = BTreeMap::<usize, f32>::new();
    for n in items {
        ensure!(
            n.size.x.is_finite() && n.size.y.is_finite() && n.size.x >= 0. && n.size.y >= 0.,
            "Invalid prepared diagram size"
        );
        let mut seen = BTreeSet::new();
        let mut current = n;
        let mut d = 0;
        while let Some(parent) = &current.parent {
            ensure!(seen.insert(parent), "Diagram cycle");
            current = by
                .get(parent)
                .ok_or_else(|| anyhow::anyhow!("Diagram orphan"))?;
            d += 1;
        }
        depth.insert(n.key.clone(), d);
        widths
            .entry(d)
            .and_modify(|w| *w = w.max(n.size.x))
            .or_insert(n.size.x);
    }
    let mut spans = BTreeMap::new();
    let mut bottom: Vec<_> = items.iter().collect();
    bottom.sort_by_key(|n| std::cmp::Reverse(depth[&n.key]));
    for n in bottom {
        let children: Vec<_> = items
            .iter()
            .filter(|c| c.parent.as_ref() == Some(&n.key))
            .collect();
        let total = children.iter().map(|c| spans[&c.key]).sum::<f32>()
            + 24. * children.len().saturating_sub(1) as f32;
        spans.insert(n.key.clone(), n.size.y.max(total));
    }
    let mut left = BTreeMap::new();
    let mut x = 24.;
    for (d, w) in widths {
        left.insert(d, x);
        x += w + 64.;
    }
    let root = items.iter().find(|n| n.parent.is_none()).unwrap();
    let mut stack = vec![(root, 24.)];
    let mut output = BTreeMap::new();
    while let Some((n, top)) = stack.pop() {
        output.insert(
            n.key.clone(),
            Rect::from_min_size(
                pos2(left[&depth[&n.key]], top + (spans[&n.key] - n.size.y) / 2.),
                n.size,
            ),
        );
        let children: Vec<_> = items
            .iter()
            .filter(|c| c.parent.as_ref() == Some(&n.key))
            .collect();
        let sum = children.iter().map(|c| spans[&c.key]).sum::<f32>()
            + 24. * children.len().saturating_sub(1) as f32;
        let mut y = top + (spans[&n.key] - sum) / 2.;
        for c in children {
            stack.push((c, y));
            y += spans[&c.key] + 24.;
        }
    }
    Ok(output)
}
#[derive(Clone)]
pub struct Camera {
    pub diagram: bool,
    pub scale: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            diagram: false,
            scale: 1.,
        }
    }
}
