//! Visual diagnostics over the frozen, validated final paint. Never a second layout engine.
use crate::{
    model::{Display, Presentation},
    painter,
};
use anyhow::{Result, anyhow, ensure};
use eframe::egui::*;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

const BLUE: Color32 = Color32::from_rgb(43, 111, 209);
const GREEN: Color32 = Color32::from_rgb(20, 143, 114);
const ORANGE: Color32 = Color32::from_rgb(195, 94, 23);

#[derive(Clone)]
pub struct Fragment {
    pub path: String,
    pub presentation: Arc<Presentation>,
    pub pipeline: Option<Value>,
}
impl Fragment {
    pub fn stage(&self, name: &str) -> Result<&Value> {
        let p = self.pipeline.as_ref().ok_or_else(|| {
            anyhow!("No pipeline captured. The final received paint is still available.")
        })?;
        ensure!(
            p["version"] == 1,
            "Unsupported pipeline version; overlays are unavailable."
        );
        let s = p["stages"]
            .as_array()
            .and_then(|a| a.iter().find(|s| s["name"] == name))
            .ok_or_else(|| anyhow!("Stage not supplied"))?;
        ensure!(
            s["status"] == "ok",
            "Stage {}: {}",
            s["status"],
            s["diagnostics"]
        );
        Ok(&s["output"])
    }
}

#[derive(Clone)]
pub struct Overlay {
    pub fragments: Vec<Fragment>,
    pub errors: Vec<String>,
    pub active: usize,
    pub selected: Option<String>,
    scene: Rect,
    bounds: bool,
    labels: bool,
    detail_key: (usize, String, Option<String>),
    detail: String,
}
impl Overlay {
    pub fn capture(root: &Presentation, received: &Value, selected: Option<String>) -> Self {
        let mut result = Self {
            fragments: vec![],
            errors: vec![],
            active: 0,
            selected,
            scene: Rect::ZERO,
            bounds: true,
            labels: false,
            detail_key: (usize::MAX, String::new(), None),
            detail: String::new(),
        };
        fn walk(root: &Presentation, v: &Value, path: String, out: &mut Overlay) {
            if v.get("display").is_some_and(|d| d.is_object())
                || v.get("readOnlyDisplay").is_some_and(|d| d.is_object())
            {
                // Same envelope inheritance as native workspace/patch fragments.
                // The derived presentation is independently validated before painting.
                let prepared = (|| -> Result<Presentation> {
                    let mut raw = serde_json::to_value(root)?;
                    for (k, value) in v.as_object().ok_or_else(|| anyhow!("Invalid fragment"))? {
                        raw[k] = value.clone();
                    }
                    if v.get("display").is_none() && v.get("readOnlyDisplay").is_some() {
                        raw["display"] = Value::Null;
                    }
                    raw["kind"] = Value::String("view".into());
                    raw["host"] = Value::Null;
                    raw["description"] = Value::Null;
                    let p = Presentation::decode(&serde_json::to_vec(&raw)?)?;
                    p.validate(&p.context, &p.snapshot.subject)?;
                    Ok(p)
                })();
                match prepared {
                    Ok(p) => out.fragments.push(Fragment {
                        path: path.clone(),
                        presentation: Arc::new(p),
                        pipeline: v.get("pipeline").cloned(),
                    }),
                    Err(e) => out.errors.push(format!("{path}: {e}")),
                }
            }
            for field in ["description", "children", "content", "contents"] {
                if let Some(a) = v[field].as_array() {
                    for (i, child) in a.iter().enumerate() {
                        walk(
                            root,
                            child,
                            format!("{}/{field}/{i}", path.trim_end_matches('/')),
                            out,
                        );
                    }
                }
            }
        }
        walk(root, received, "/".into(), &mut result);
        if let Some(id) = &result.selected
            && let Some(i) = result.fragments.iter().position(|f| {
                f.presentation
                    .display
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|n| &n.id == id)
            })
        {
            result.active = i;
        }
        result
    }
    pub fn sidebar(&mut self, ui: &mut Ui, stage: &str) {
        if !self.errors.is_empty() {
            ui.collapsing(
                format!("{} unavailable fragments", self.errors.len()),
                |ui| {
                    for e in &self.errors {
                        ui.colored_label(ORANGE, e);
                    }
                },
            );
        }
        if self.fragments.len() > 1 {
            ui.strong("Painted fragment");
            let before = self.active;
            ComboBox::from_id_salt("overlay-fragment")
                .selected_text(&self.fragments[self.active].path)
                .show_ui(ui, |ui| {
                    for (i, f) in self.fragments.iter().enumerate() {
                        ui.selectable_value(&mut self.active, i, &f.path);
                    }
                });
            if before != self.active {
                self.selected = None;
                self.scene = Rect::ZERO;
            }
            ui.small("Fragments keep their own coordinate spaces.");
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.bounds, "Bounds");
            ui.checkbox(&mut self.labels, "IDs");
        });
        ui.horizontal(|ui| {
            if ui.button("Fit view").clicked() {
                self.scene = Rect::ZERO;
            }
            if ui.button("Clear selection").clicked() {
                self.selected = None;
            }
        });
        ui.small("Click to inspect · drag to pan · pinch or Ctrl-scroll to zoom");
        ui.separator();
        let Some(f) = self.fragments.get(self.active) else {
            ui.label("No supported prepared display in this response.");
            for e in &self.errors {
                ui.colored_label(ORANGE, e);
            }
            return;
        };
        let key = (self.active, stage.to_owned(), self.selected.clone());
        if self.detail_key != key {
            self.detail_key = key;
            self.detail = match (self.selected.as_deref(), f.stage(stage)) {
                (Some(id), Ok(output)) if has_ids(stage) => {
                    let selected = match stage {
                        "metrics" => serde_json::json!({"id": id, "size": output["sizes"][id], "width": output["width"]}),
                        "trace" => Value::Array(output.as_array().into_iter().flatten().filter(|v| v["leftId"] == id || v["rightId"] == id || v["parent"] == id).cloned().collect()),
                        "bindings" => Value::Array(output.as_array().into_iter().flatten().filter(|v| v["binding"]["nodeId"] == id).cloned().collect()),
                        "prepared" => serde_json::json!({"context":output["context"], "lines": super::identity_records(&output["lines"], id)}),
                        _ => Value::Array(super::identity_records(output, id)),
                    };
                    super::pretty::format_stage(stage, &selected)
                }
                (_, Err(e)) => e.to_string(),
                (Some(_), _) => "This stage has no explicit node mapping. Use Pipeline text for the whole stage.".into(),
                _ => "Click a painted region to inspect its stage data.".into(),
            };
        }
        if let Some(id) = &self.selected {
            ui.strong("Selected region");
            ui.small(id);
            if let Some(n) = f
                .presentation
                .display
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|n| &n.id == id)
            {
                ui.label(format!("{} · {}", n.kind, n.semantics.label));
                ui.small(format!(
                    "x {:.2}  y {:.2}  ·  {:.2} × {:.2} CSS px",
                    n.r#box.x, n.r#box.y, n.r#box.width, n.r#box.height
                ));
                if let Some(parent) = &n.parent_id
                    && ui.button("Select parent").clicked()
                {
                    self.selected = Some(parent.clone());
                }
            }
        }
        let mut text = self.detail.as_str();
        ui.add(
            TextEdit::multiline(&mut text)
                .font(TextStyle::Monospace)
                .frame(false)
                .desired_width(ui.available_width())
                .desired_rows(1),
        );
    }
    pub fn show(&mut self, ui: &mut Ui, stage: &str) {
        let Some(f) = self.fragments.get(self.active) else {
            ui.heading("Painted preview unavailable");
            for e in &self.errors {
                ui.label(e);
            }
            ui.label("Use the received JSON to inspect unsupported structural output.");
            return;
        };
        let d = f.presentation.display.as_ref().unwrap();
        ui.heading(format!("{stage} / Painted view"));
        ui.small(format!(
            "Frozen received values · {:.0} × {:.0} CSS px · controls are non-editable",
            d.width, d.height
        ));
        let output = f.stage(stage);
        if !has_ids(stage) {
            ui.small("Final paint reference only. This stage has no node-ID mapping; no geometry is inferred.");
        } else {
            ui.small(match stage {
                "metrics" => "Select a region: green = minimum width, orange = preferred width.",
                "prepared" => "Green = final-display line boxes · orange = final baselines. Stage wrapping details appear in the sidebar.",
                "layout" => "Blue bands = selected container’s rows, located using its children’s final boxes. Labels show supplied gaps.",
                "trace" | "semantic" => "Orange arrows = declared adjacency, placed between final boxes.",
                "bindings" => "Green = presented control bindings. Select a region for route and captures.",
                _ => "Blue = node bounds · orange = selected region. Click a region for stage details.",
            });
        }
        if let Err(e) = &output {
            ui.colored_label(ORANGE, e.to_string());
        }
        ui.add_space(12.);
        let marks = output
            .as_ref()
            .ok()
            .map(|v| annotations(d, stage, v, self.selected.as_deref()))
            .unwrap_or_default();
        let stage_ids = output
            .as_ref()
            .ok()
            .filter(|_| ["normalized", "visible", "semantic"].contains(&stage))
            .map(|v| ids(v));
        if let Some(ids) = &stage_ids {
            let missing = ids
                .iter()
                .filter(|id| !d.nodes.iter().any(|n| &n.id == *id))
                .count();
            if missing > 0 {
                ui.small(format!(
                    "{missing} stage node IDs have no final painted box; see Pipeline text."
                ));
            }
        }
        let mut pan = Vec2::ZERO;
        Scene::new()
            .drag_pan_buttons(Default::default())
            .zoom_range(0.05..=4.)
            .max_inner_size(vec2(d.width as f32 + 48., d.height as f32 + 48.))
            .show(ui, &mut self.scene, |ui| {
                // One scene unit is one CSS pixel; the Scene applies zoom uniformly.
                let (canvas, _) = ui.allocate_exact_size(
                    vec2(d.width as f32 + 48., d.height as f32 + 48.),
                    Sense::hover(),
                );
                let mut paint_ui = ui.new_child(UiBuilder::new().max_rect(canvas.shrink(24.)));
                let rect = painter::snapshot(&mut paint_ui, &f.presentation);
                let origin = rect.min.to_vec2();
                let response = ui.interact(
                    ui.clip_rect(),
                    ui.id().with("pick-region"),
                    Sense::click_and_drag(),
                );
                pan = response.drag_delta();
                let hovered = response.hover_pos().and_then(|p| hit_node(d, p - origin));
                if response.clicked() {
                    self.selected = hovered.clone();
                }
                if let Some(id) = &hovered {
                    response.on_hover_text(format!("{}\nClick to pin stage details", id));
                }
                let p = ui
                    .painter()
                    .with_clip_rect(ui.clip_rect().intersect(canvas));
                for n in &d.nodes {
                    let r = node_rect(n).translate(origin);
                    let selected = self.selected.as_deref() == Some(&n.id);
                    let hovered = hovered.as_deref() == Some(&n.id);
                    let mapped = stage_ids.as_ref().is_none_or(|ids| ids.contains(&n.id));
                    if (self.bounds && mapped) || selected || hovered {
                        let color = if selected || hovered {
                            ORANGE
                        } else {
                            BLUE.gamma_multiply(0.45)
                        };
                        p.rect_stroke(
                            r,
                            0.,
                            Stroke::new(if selected { 2_f32 } else { 1_f32 }, color),
                            StrokeKind::Inside,
                        );
                    }
                    if self.labels && mapped || selected {
                        badge(
                            &p,
                            r.left_top() - vec2(0., 17.),
                            &format!("{} · {}", n.id.rsplit(':').next().unwrap_or(&n.id), n.kind),
                            if selected { ORANGE } else { BLUE },
                        );
                    }
                }
                for m in &marks {
                    if let Some(r) = m.rect {
                        let r = r.translate(origin);
                        p.rect_filled(r, 0., m.color.gamma_multiply(0.10));
                        p.rect_stroke(r, 0., Stroke::new(1_f32, m.color), StrokeKind::Inside);
                        if !m.label.is_empty() {
                            badge(&p, r.left_bottom(), &m.label, m.color);
                        }
                    }
                    if let Some([a, b]) = m.line {
                        let a = a + origin;
                        let b = b + origin;
                        if stage == "prepared" {
                            p.line_segment([a, b], Stroke::new(1_f32, m.color));
                        } else {
                            p.arrow(a, b - a, Stroke::new(1.5_f32, m.color));
                        }
                        if !m.label.is_empty() {
                            badge(&p, a.lerp(b, 0.5), &m.label, m.color);
                        }
                    }
                }
            });
        if pan != Vec2::ZERO {
            self.scene = self.scene.translate(-pan);
            ui.ctx().request_repaint();
        }
    }
}

fn badge(p: &Painter, pos: Pos2, text: &str, color: Color32) {
    let g = p.layout_no_wrap(text.to_owned(), FontId::monospace(10.), Color32::WHITE);
    let r = Rect::from_min_size(pos, g.size() + vec2(6., 4.));
    p.rect_filled(r, 2., color);
    p.galley(pos + vec2(3., 2.), g, Color32::WHITE);
}
fn node_rect(n: &crate::model::Node) -> Rect {
    Rect::from_min_size(
        pos2(n.r#box.x as f32, n.r#box.y as f32),
        vec2(n.r#box.width as f32, n.r#box.height as f32),
    )
}
fn has_ids(stage: &str) -> bool {
    !["parse", "read", "source", "core", "html"].contains(&stage)
}
fn ids(v: &Value) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        if let Some(id) = v["id"].as_str().or(v["attrs"]["id"].as_str()) {
            out.insert(id.to_owned());
        }
        match v {
            Value::Object(m) => m.values().for_each(|v| walk(v, out)),
            Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => (),
        }
    }
    walk(v, &mut result);
    result
}
/// Pick in paint order and honor all ancestor clips, including rounded outlines.
pub fn hit_node(d: &Display, point: Pos2) -> Option<String> {
    let mut clips = BTreeMap::<&str, Vec<Vec<Pos2>>>::new();
    let mut picked = None;
    for n in &d.nodes {
        let mut inherited = n
            .parent_id
            .as_deref()
            .and_then(|id| clips.get(id))
            .cloned()
            .unwrap_or_default();
        let r = node_rect(n);
        if n.paint.clip {
            inherited.push(crate::paint_geometry::outline(r, &n.paint.corners));
        }
        if r.contains(point)
            && inherited
                .iter()
                .all(|p| crate::paint_geometry::contains(point, p))
        {
            picked = Some(n.id.clone());
        }
        clips.insert(&n.id, inherited);
    }
    picked
}

#[derive(Debug)]
pub struct Mark {
    pub rect: Option<Rect>,
    pub line: Option<[Pos2; 2]>,
    pub label: String,
    pub color: Color32,
}
/// Geometry only from final supplied boxes/lines. Earlier stage values label it.
pub fn annotations(d: &Display, stage: &str, output: &Value, selected: Option<&str>) -> Vec<Mark> {
    let nodes: BTreeMap<_, _> = d.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut marks = vec![];
    let mut rect = |r, label, color| {
        marks.push(Mark {
            rect: Some(r),
            line: None,
            label,
            color,
        })
    };
    match stage {
        "metrics" => {
            if let Some(n) = selected.and_then(|id| nodes.get(id)) {
                let b = node_rect(n);
                for (key, color, dy) in [("min", GREEN, 0.), ("pref", ORANGE, 5.)] {
                    if let Some(w) = output["sizes"][&n.id][key]
                        .as_f64()
                        .filter(|w| w.is_finite() && (0. ..1e6).contains(w))
                    {
                        rect(
                            Rect::from_min_size(
                                b.left_bottom() - vec2(0., dy + 4.),
                                vec2(w as f32, 3.),
                            ),
                            format!("{key} {w:.2}"),
                            color,
                        );
                    }
                }
            }
        }
        "prepared" => {
            let supplied = ids(&output["lines"]);
            for n in &d.nodes {
                if !supplied.contains(&n.id) {
                    continue;
                }
                for l in &n.lines {
                    let b = &l.r#box;
                    rect(
                        Rect::from_min_size(
                            pos2(b.x as f32, b.y as f32),
                            vec2(b.width as f32, b.height as f32),
                        ),
                        String::new(),
                        GREEN,
                    );
                }
            }
            for n in &d.nodes {
                if !supplied.contains(&n.id) {
                    continue;
                }
                for l in &n.lines {
                    marks.push(Mark {
                        rect: None,
                        line: Some([
                            pos2(l.r#box.x as f32, l.baseline as f32),
                            pos2((l.r#box.x + l.r#box.width) as f32, l.baseline as f32),
                        ]),
                        label: String::new(),
                        color: ORANGE,
                    });
                }
            }
        }
        "layout" => {
            let id = selected.or_else(|| d.nodes.first().map(|n| n.id.as_str()));
            if let Some(plan) =
                id.and_then(|id| super::identity_records(output, id).into_iter().next())
                && let Some(rows) = plan["rows"].as_array()
            {
                for (i, row) in rows.iter().enumerate() {
                    let children: Vec<_> = row["children"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|c| c["id"].as_str().and_then(|id| nodes.get(id)))
                        .collect();
                    if !children.is_empty()
                        && row["children"]
                            .as_array()
                            .is_some_and(|a| a.len() == children.len())
                    {
                        let r = children
                            .iter()
                            .fold(Rect::NOTHING, |r, n| r.union(node_rect(n)));
                        rect(r, format!("row {} · gap {}", i + 1, row["gap"]), BLUE);
                    }
                }
            }
        }
        "bindings" => {
            for b in output
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| &c["binding"])
            {
                if b["presented"] == true
                    && let Some(n) = b["nodeId"].as_str().and_then(|id| nodes.get(id))
                {
                    rect(
                        node_rect(n),
                        b["route"].as_str().unwrap_or("binding").to_owned(),
                        GREEN,
                    );
                }
            }
        }
        "trace" | "semantic" => {
            let mut edges = vec![];
            fn walk<'a>(v: &'a Value, edges: &mut Vec<&'a Value>) {
                if v.get("leftId").is_some() && v.get("rightId").is_some() {
                    edges.push(v);
                    return;
                }
                match v {
                    Value::Object(m) => m.values().for_each(|v| walk(v, edges)),
                    Value::Array(a) => a.iter().for_each(|v| walk(v, edges)),
                    _ => (),
                }
            }
            walk(output, &mut edges);
            for e in edges {
                if selected
                    .is_some_and(|id| e["leftId"] != id && e["rightId"] != id && e["parent"] != id)
                {
                    continue;
                }
                let (Some(a), Some(b)) = (
                    e["leftId"].as_str().and_then(|id| nodes.get(id)),
                    e["rightId"].as_str().and_then(|id| nodes.get(id)),
                ) else {
                    continue;
                };
                let a = node_rect(a);
                let b = node_rect(b);
                let [start, end] = if stage == "semantic" || e["axis"].is_null() {
                    [a.center(), b.center()]
                } else if e["axis"] == "x" {
                    [a.right_center(), b.left_center()]
                } else {
                    [a.center_bottom(), b.center_top()]
                };
                marks.push(Mark {
                    rect: None,
                    line: Some([start, end]),
                    label: if stage == "trace" {
                        format!("{} · gap {}", e["bind"].as_str().unwrap_or("?"), e["gap"])
                    } else {
                        e["bind"].as_str().unwrap_or("adjacency").to_owned()
                    },
                    color: ORANGE,
                });
            }
        }
        _ => (),
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canvas_drag_and_zoom_transform_the_frozen_scene() {
        let raw: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/live-pipeline.json")).unwrap();
        let p = Presentation::decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let mut overlay = Overlay::capture(&p, &raw, None);
        let ctx = Context::default();
        painter::install(&ctx);
        let frame = |overlay: &mut Overlay, events: Vec<Event>| {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 700.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| overlay.show(ui, "display"));
                },
            );
        };
        frame(&mut overlay, vec![]);
        frame(&mut overlay, vec![]);
        let before = overlay.scene;
        let start = pos2(500., 400.);
        let end = start + vec2(60., 20.);
        frame(
            &mut overlay,
            vec![
                Event::PointerMoved(start),
                Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(&mut overlay, vec![Event::PointerMoved(end)]);
        frame(
            &mut overlay,
            vec![Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(
            overlay.scene.min.x < before.min.x,
            "drag did not pan: {:?} vs {:?}",
            overlay.scene,
            before
        );
        assert!(overlay.selected.is_none(), "a drag must not pick a region");
        let before_zoom = overlay.scene;
        frame(
            &mut overlay,
            vec![Event::PointerMoved(end), Event::Zoom(1.5)],
        );
        assert!(
            overlay.scene.width() < before_zoom.width(),
            "zoom did not scale the scene"
        );
        assert_eq!(overlay.fragments[0].presentation.context.width, 480.);
    }
}
