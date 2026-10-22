//! Read-only, frozen observations of the mounted response. No invocation authority.
pub mod overlay;
pub mod pretty;
use crate::session::Editor;
use anyhow::{Result, anyhow};
use eframe::egui::{self, *};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

// Capture is a process-wide diagnostic preference, never invocation state.
static CAPTURE_PIPELINE: AtomicBool = AtomicBool::new(false);
pub fn capture_pipeline() -> bool {
    CAPTURE_PIPELINE.load(Ordering::Relaxed)
}
pub fn pipeline_toggle(ui: &mut Ui) -> bool {
    let mut enabled = capture_pipeline();
    let changed = ui.checkbox(&mut enabled, "Capture pipeline")
        .on_hover_text("Include compiler stages in subsequent render responses. Refresh, then inspect a component. Larger responses; local drafts are never sent.").changed();
    if changed {
        CAPTURE_PIPELINE.store(enabled, Ordering::Relaxed);
    }
    changed
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Overlay,
    View,
    Component,
    Pipeline,
    Response,
}
use uuid::Uuid;

#[derive(Clone)]
pub struct Inspection {
    pub owner: Uuid,
    pub generation: u64,
    pub target: String,
    pub source: Arc<str>,
    pub received: Value,
    pub selected: Option<String>,
    component_json: String,
    view_json: String,
    full_json: String,
    tab: Tab,
    stage: String,
    component_only: bool,
    stage_json: String,
    stage_pretty: String,
    raw_stage: bool,
    overlay: overlay::Overlay,
    verbatim: bool,
}
impl Inspection {
    pub fn capture(e: &Editor, selected: Option<String>) -> Result<Self> {
        let p = e
            .presentation
            .as_ref()
            .ok_or_else(|| anyhow!("No mounted Goo response"))?;
        let source = if let Some(bytes) = &p.received_protobuf {
            serde_json::to_string(&crate::protobuf::decode(bytes)?)?.into()
        } else {
            p.received_json
                .clone()
                .ok_or_else(|| anyhow!("Original response was not retained"))?
        };
        let received: Value = serde_json::from_str(&source)?;
        let picked = selected.clone();
        let selected = selected.or_else(|| {
            records(&received)
                .first()
                .and_then(|(n, _)| n["id"].as_str().map(str::to_owned))
        });
        let view_json = serde_json::to_string_pretty(&prepared_view(&received))?;
        let overlay = overlay::Overlay::capture(p, &received, picked);
        let mut result = Self {
            owner: e.id,
            generation: e.mounted_generation,
            target: e.target.clone(),
            source,
            full_json: serde_json::to_string_pretty(&received)?,
            received,
            selected: None,
            component_json: String::new(),
            view_json,
            tab: Tab::Overlay,
            stage: "display".into(),
            component_only: false,
            stage_json: String::new(),
            stage_pretty: String::new(),
            raw_stage: false,
            overlay,
            verbatim: false,
        };
        result.select(selected);
        Ok(result)
    }
    /// Read from the original JSON so unknown server fields survive inspection.
    pub fn component(&self, id: &str) -> Option<Value> {
        let (node, source) = records(&self.received)
            .into_iter()
            .find(|(n, _)| n["id"] == id)?;
        let controls: Vec<_> = source["controls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["binding"]["nodeId"] == id)
            .cloned()
            .collect();
        Some(json!({"node": node, "controls": controls}))
    }
    pub fn select(&mut self, id: Option<String>) {
        self.component_json = id
            .as_deref()
            .and_then(|id| self.component(id))
            .map(|v| serde_json::to_string_pretty(&v).unwrap())
            .unwrap_or_else(|| {
                "No prepared node selected. View the full response for the envelope.".into()
            });
        self.selected = id;
        self.update_stage();
    }
    /// The selected node's own compilation, including nested patch/pane bodies.
    pub fn pipeline(&self) -> Option<&Value> {
        let container = self
            .selected
            .as_deref()
            .and_then(|id| {
                records(&self.received)
                    .into_iter()
                    .find(|(n, _)| n["id"] == id)
                    .map(|(_, c)| c)
            })
            .unwrap_or(&self.received);
        container.get("pipeline")
    }
    pub fn stages(&self) -> Result<&[Value]> {
        let p = self.pipeline().ok_or_else(|| anyhow!("No pipeline captured for this view. Enable Capture pipeline, refresh the view, then inspect again. Older servers and structural host components may not supply stages."))?;
        anyhow::ensure!(
            p["version"] == 1,
            "Unsupported pipeline version; inspect the full response for raw data."
        );
        p["stages"]
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| anyhow!("Malformed pipeline: missing stages"))
    }
    /// Filter only by explicit compiler identities. Never infer pre-ID correspondence.
    pub fn stage_output(&self, name: &str, component_only: bool) -> Result<Value> {
        let stage = self
            .stages()?
            .iter()
            .find(|s| s["name"] == name)
            .ok_or_else(|| anyhow!("Stage was not supplied by this server"))?;
        if !component_only || ["parse", "read", "source", "core", "html"].contains(&name) {
            return Ok(stage["output"].clone());
        }
        let id = self
            .selected
            .as_deref()
            .ok_or_else(|| anyhow!("No component selected"))?;
        let output = &stage["output"];
        Ok(match name {
            "metrics" => json!({"id": id, "size": output["sizes"][id], "width": output["width"]}),
            "trace" => Value::Array(
                output
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|v| v["leftId"] == id || v["rightId"] == id || v["parent"] == id)
                    .cloned()
                    .collect(),
            ),
            "bindings" => Value::Array(
                output
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|v| v["binding"]["nodeId"] == id)
                    .cloned()
                    .collect(),
            ),
            "prepared" => {
                json!({"context":output["context"], "lines": identity_records(&output["lines"], id)})
            }
            "normalized" | "visible" | "semantic" | "layout" | "display" => {
                Value::Array(identity_records(output, id))
            }
            _ => output.clone(),
        })
    }
    /// Whole-view inspection retains every separately prepared fragment, with
    /// its received JSON path. It never invents a combined compiler tree.
    pub fn view_stage_output(&self, name: &str) -> Result<Value> {
        let captures = pipelines(&self.received);
        anyhow::ensure!(
            !captures.is_empty(),
            "No pipeline captured for this view. Enable Capture pipeline, refresh, then inspect again."
        );
        let mut outputs = Vec::new();
        for (path, pipeline) in &captures {
            anyhow::ensure!(
                pipeline["version"] == 1,
                "Unsupported pipeline version at {path}; inspect the full response for raw data."
            );
            let stages = pipeline["stages"]
                .as_array()
                .ok_or_else(|| anyhow!("Malformed pipeline at {path}"))?;
            let stage = stages.iter().find(|stage| stage["name"] == name);
            if captures.len() == 1 {
                return stage
                    .map(|s| s["output"].clone())
                    .ok_or_else(|| anyhow!("Stage was not supplied by this server"));
            }
            outputs.push(json!({"fragment": path, "status": stage.map(|s| s["status"].clone()).unwrap_or(json!("not supplied")),
                "output": stage.map(|s| s["output"].clone()), "diagnostics": stage.map(|s| s["diagnostics"].clone())}));
        }
        Ok(Value::Array(outputs))
    }
    fn update_stage(&mut self) {
        let output = if self.component_only {
            self.stage_output(&self.stage, true)
        } else {
            self.view_stage_output(&self.stage)
        };
        match output {
            Ok(value) => {
                self.stage_pretty = pretty::format_stage(&self.stage, &value);
                self.stage_json = match value {
                    Value::String(text) => text,
                    value => serde_json::to_string_pretty(&value).unwrap(),
                };
            }
            Err(error) => {
                self.stage_json = error.to_string();
                self.stage_pretty = self.stage_json.clone();
            }
        }
    }
    fn pipeline_text(&self) -> &str {
        if self.raw_stage {
            &self.stage_json
        } else {
            &self.stage_pretty
        }
    }
    fn sidebar(&mut self, ui: &mut Ui) {
        ui.style_mut().wrap_mode = Some(TextWrapMode::Wrap);
        ui.strong("View");
        ui.label(self.received["view"].as_str().unwrap_or("Prepared view"));
        ui.small(&self.target);
        ui.separator();
        ui.selectable_value(&mut self.tab, Tab::Overlay, "Visual pipeline");
        ui.selectable_value(&mut self.tab, Tab::View, "Prepared view");
        ui.selectable_value(&mut self.tab, Tab::Pipeline, "Pipeline text");
        ui.selectable_value(&mut self.tab, Tab::Response, "Full response");
        ui.selectable_value(&mut self.tab, Tab::Component, "Render node details");
        if self.tab == Tab::Response {
            ui.checkbox(&mut self.verbatim, "Compact JSON");
        }
        if self.tab == Tab::Pipeline {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.raw_stage, false, "Readable");
                ui.selectable_value(&mut self.raw_stage, true, "Raw JSON");
            });
        }
        if ui
            .button(if self.tab == Tab::Response {
                "Copy response as JSON"
            } else {
                "Copy output"
            })
            .clicked()
        {
            ui.ctx().copy_text(match self.tab {
                Tab::Overlay => self
                    .overlay
                    .fragments
                    .get(self.overlay.active)
                    .and_then(|f| f.stage(&self.stage).ok())
                    .map(|v| pretty::format_stage(&self.stage, v))
                    .unwrap_or_else(|| "No pipeline stage captured".into()),
                Tab::Response => self.source.to_string(),
                Tab::View => self.view_json.clone(),
                Tab::Component => self.component_json.clone(),
                Tab::Pipeline => self.pipeline_text().to_owned(),
            });
        }
        ui.separator();
        if self.tab == Tab::Pipeline || self.tab == Tab::Overlay {
            ui.strong("Stage");
            let stages = if self.tab == Tab::Overlay {
                self.overlay
                    .fragments
                    .get(self.overlay.active)
                    .and_then(|f| f.pipeline.as_ref())
                    .filter(|p| p["version"] == 1)
                    .and_then(|p| p["stages"].as_array())
                    .map(Vec::as_slice)
                    .unwrap_or_default()
            } else {
                self.stages().unwrap_or_default()
            };
            let stages: Vec<_> = stages
                .iter()
                .map(|s| {
                    (
                        s["name"].as_str().unwrap_or("?").to_owned(),
                        s["status"].as_str().unwrap_or("?").to_owned(),
                    )
                })
                .collect();
            let selected = stages
                .iter()
                .find(|(name, _)| name == &self.stage)
                .map(|(name, status)| format!("{name} · {status}"))
                .unwrap_or_else(|| self.stage.clone());
            let mut changed = false;
            ComboBox::from_id_salt("goo-stage")
                .width(ui.available_width())
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (name, status) in stages {
                        let label = format!("{name} · {status}");
                        changed |= ui.selectable_value(&mut self.stage, name, label).changed();
                    }
                });
            if self.tab == Tab::Pipeline {
                changed |= ui
                    .checkbox(&mut self.component_only, "Inspect one render node")
                    .changed();
            }
            if changed {
                self.update_stage();
            }
            if self.tab == Tab::Pipeline && !self.component_only {
                let count = pipelines(&self.received).len();
                ui.small(if count > 1 {
                    format!("Whole view · {count} separately prepared fragments")
                } else {
                    "Whole view · complete stage output".into()
                });
            }
            ui.small(stage_description(&self.stage));
            if ["parse", "read", "source", "core"].contains(&self.stage.as_str()) {
                ui.small("Whole stage: component IDs are not available here.");
            } else if self.stage == "html" {
                ui.small("Whole HTML projection, shown as text.");
            }
            if self.tab == Tab::Pipeline
                && let Ok(stages) = self.stages()
                && let Some(stage) = stages.iter().find(|s| s["name"] == self.stage)
            {
                if stage["status"] == "skipped" {
                    ui.small("Not captured by live preparation.");
                }
                if let Some(diagnostics) = stage["diagnostics"].as_array() {
                    for d in diagnostics {
                        ui.label(d.to_string());
                    }
                }
            }
            ui.separator();
        }
        if self.tab == Tab::Overlay {
            self.overlay.sidebar(ui, &self.stage);
            ui.separator();
        }
        if self.tab == Tab::Component || (self.tab == Tab::Pipeline && self.component_only) {
            ui.strong("Render nodes");
            let mut chosen = None;
            for (node, _) in records(&self.received) {
                let Some(id) = node["id"].as_str() else {
                    continue;
                };
                let kind = node["kind"].as_str().unwrap_or("?");
                let label = node["semantics"]["label"]
                    .as_str()
                    .filter(|s| !s.is_empty());
                let title = label
                    .map(|s| format!("{kind} · {s}"))
                    .unwrap_or_else(|| format!("{kind} · {}", id.rsplit(':').next().unwrap_or(id)));
                if ui
                    .add(Button::selectable(self.selected.as_deref() == Some(id), title).wrap())
                    .on_hover_text(id)
                    .clicked()
                {
                    chosen = Some(id.to_owned());
                }
            }
            if let Some(id) = chosen {
                self.select(Some(id));
            }
            if let Some(id) = &self.selected {
                ui.small(format!("Selected: {id}"));
            }
            ui.separator();
        }
        if self.tab == Tab::View {
            ui.small("Complete prepared view and bindings. Pipeline and HTML are available in their own views.");
        }
        pipeline_toggle(ui);
        ui.small("Refresh, then inspect again to capture future renders.");
        ui.separator();
        ui.collapsing("Capture details", |ui| {
            ui.label(format!("Frozen generation {}", self.generation));
            ui.small(format!("Editor {}", self.owner));
            ui.small(&self.target);
            ui.small(format!(
                "View {}",
                self.received["view"].as_str().unwrap_or("?")
            ));
            ui.small(format!(
                "Publication {}",
                self.received["snapshot"]["publication"]
                    .as_str()
                    .unwrap_or("?")
            ));
            if let Some(note) = self.pipeline().and_then(|p| p["note"].as_str()) {
                ui.small(note);
            }
            ui.small("Local drafts are separate. Inspection does not grant action authority.");
        });
    }
    pub fn show(&mut self, ctx: &egui::Context, open: &mut bool) {
        Window::new("Received Goo")
            .id(Id::new("goo-inspector"))
            .open(open)
            .default_size([1100., 720.])
            .min_size([680., 400.])
            .show(ctx, |ui| {
                // Both panels use the window's bounded height. Scrolling either
                // one never pushes the other below the fold or grows the window.
                ui.set_min_size(ui.available_size());
                let max_sidebar = (ui.available_width() * 0.4).clamp(220., 400.);
                SidePanel::left("goo-inspector-sidebar")
                    .resizable(true)
                    .default_width(280.)
                    .width_range(220. ..=max_sidebar)
                    .frame(
                        Frame::new()
                            .fill(ui.visuals().faint_bg_color)
                            .inner_margin(12.),
                    )
                    .show_inside(ui, |ui| {
                        ScrollArea::vertical()
                            .id_salt("goo-inspector-controls")
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.sidebar(ui));
                    });
                CentralPanel::default()
                    .frame(Frame::new().fill(ui.visuals().panel_fill).inner_margin(12.))
                    .show_inside(ui, |ui| {
                        if self.tab == Tab::Overlay {
                            self.overlay.show(ui, &self.stage);
                            return;
                        }
                        if self.tab == Tab::Pipeline && !self.raw_stage {
                            ui.heading(format!("{} / {}", self.stage, if self.component_only { "Render node" } else { "Whole view" }));
                            ui.small("Compact diagnostic view · numbers rounded to 3 decimals · Raw JSON retains every field");
                            ui.add_space(10.);
                        }
                        let width = ui.available_width();
                        ScrollArea::both()
                            .id_salt((
                                "goo-json",
                                self.tab as u8,
                                &self.selected,
                                &self.stage,
                                self.component_only,
                                self.verbatim,
                                self.raw_stage,
                            ))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let mut text: &str = match self.tab {
                                    Tab::Overlay => unreachable!(),
                                    Tab::Response => {
                                        if self.verbatim {
                                            &self.source
                                        } else {
                                            &self.full_json
                                        }
                                    }
                                    Tab::View => &self.view_json,
                                    Tab::Component => &self.component_json,
                                    Tab::Pipeline => self.pipeline_text(),
                                };
                                ui.add(
                                    TextEdit::multiline(&mut text)
                                        .id_salt("goo-inspected-output")
                                        .font(TextStyle::Monospace)
                                        .frame(false)
                                        .desired_rows(1)
                                        .desired_width(width),
                                );
                            });
                    });
            });
    }
}

fn records(value: &Value) -> Vec<(&Value, &Value)> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<(&'a Value, &'a Value)>) {
        for name in ["display", "readOnlyDisplay"] {
            if let Some(nodes) = v[name]["nodes"].as_array() {
                out.extend(nodes.iter().map(|n| (n, v)));
            }
        }
        for name in ["description", "children", "contents", "content"] {
            if let Some(items) = v[name].as_array() {
                for child in items {
                    walk(child, out);
                }
            }
        }
    }
    let mut result = vec![];
    walk(value, &mut result);
    result
}

fn identity_records(value: &Value, id: &str) -> Vec<Value> {
    let mut matches = vec![];
    fn walk(v: &Value, id: &str, out: &mut Vec<Value>) {
        if v.is_object() && (v["id"] == id || v["attrs"]["id"] == id) {
            out.push(v.clone());
            return;
        }
        match v {
            Value::Object(fields) => {
                for value in fields.values() {
                    walk(value, id, out);
                }
            }
            Value::Array(values) => {
                for value in values {
                    walk(value, id, out);
                }
            }
            _ => (),
        }
    }
    walk(value, id, &mut matches);
    matches
}
fn stage_description(name: &str) -> &'static str {
    match name {
        "source" => "Evaluated typed Goo: receiver values and defaults have been bound.",
        "core" => "Source sugar normalized; optional ranks propagated.",
        "normalized" => "Stable node IDs allocated before pruning.",
        "visible" => "Nodes remaining after visibility and budget rules.",
        "semantic" => "Semantic adjacency and resolved relationship rules.",
        "metrics" => "Measured minimum/preferred sizes and available width.",
        "layout" => "Chosen row breaks, allocations, heights and cost.",
        "trace" => "Layout decisions: gaps, stretch, break penalties and rules.",
        "prepared" => "Text wrapping and measured lines used by preparation.",
        "display" => {
            "Portable boxes, baselines, fonts, resolved paint and accessibility semantics."
        }
        "bindings" => "Control-to-node joins. Inspection does not grant action authority.",
        "html" => "Server HTML projection; the native renderer does not parse it.",
        _ => "Server-reported compiler stage.",
    }
}

// Only descend into actual render-fragment containers, never arbitrary metadata.
fn pipelines(value: &Value) -> Vec<(String, &Value)> {
    fn walk<'a>(v: &'a Value, path: String, out: &mut Vec<(String, &'a Value)>) {
        if let Some(pipeline) = v.get("pipeline") {
            out.push((path.clone(), pipeline));
        }
        for name in ["description", "children", "contents", "content"] {
            if let Some(items) = v[name].as_array() {
                for (index, child) in items.iter().enumerate() {
                    walk(child, format!("{path}/{name}/{index}"), out);
                }
            }
        }
    }
    let mut result = vec![];
    walk(value, String::new(), &mut result);
    result
}
fn prepared_view(value: &Value) -> Value {
    let mut result = value.clone();
    if let Some(fields) = result.as_object_mut() {
        fields.remove("pipeline");
        fields.remove("html");
        for name in ["description", "children", "contents", "content"] {
            if let Some(Value::Array(items)) = fields.get_mut(name) {
                for child in items {
                    *child = prepared_view(child);
                }
            }
        }
    }
    result
}
