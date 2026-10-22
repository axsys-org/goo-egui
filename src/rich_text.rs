//! Structured native editing. Metadata travels with characters; paint comes from Reaver.
use crate::model::*;
use anyhow::{Result, ensure};
use eframe::egui::{self, *};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{cell::RefCell, ops::Range, rc::Rc};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Editing {
    pub version: u32,
    pub encoding: String,
    pub max_bytes: usize,
    pub styles: Vec<EditStyle>,
    pub link_style: ResolvedRun,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditStyle {
    pub myth: RunDescriptor,
    pub resolved: ResolvedRun,
}
pub fn style_key(m: &RunDescriptor) -> RunDescriptor {
    let mut m = m.clone();
    m.remove("/sys/link");
    if let Some(p) = m.get_mut("/sys/modifiers") {
        if let Some(v) = p["value"].as_array_mut() {
            v.sort_by_key(|v| v.as_str().unwrap_or("").to_owned());
        }
        if p["value"].as_array().is_some_and(Vec::is_empty) {
            m.remove("/sys/modifiers");
        }
    }
    m
}
impl Editing {
    pub fn style(&self, m: &RunDescriptor) -> Option<&ResolvedRun> {
        let key = style_key(m);
        self.styles
            .iter()
            .find(|s| style_key(&s.myth) == key)
            .map(|s| &s.resolved)
    }
    pub fn decode(&self, wire: &str) -> Result<Vec<TextRun>> {
        ensure!(
            wire.len() <= self.max_bytes,
            "Rich text exceeds the editing limit; draft retained"
        );
        let runs: Vec<TextRun> = serde_json::from_str(wire)?;
        ensure!(runs.len() <= 10000, "Too many rich-text runs");
        for run in &runs {
            crate::model::validate_run_myth(&run.myth)?;
            ensure!(
                ascii(run.lang.text()),
                "This server supports ASCII text; draft retained"
            );
            ensure!(
                self.style(&run.myth).is_some(),
                "Unsupported rich-text style; draft retained"
            );
        }
        Ok(runs)
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Atom {
    ch: char,
    myth: RunDescriptor,
}
#[derive(Clone, Debug)]
struct Snapshot {
    atoms: Vec<Atom>,
    selection: Range<usize>,
    typing: Option<RunDescriptor>,
}
#[derive(Clone, Debug)]
pub struct Draft {
    atoms: Vec<Atom>,
    text: String,
    pub selection: Range<usize>,
    typing: Option<RunDescriptor>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    composition_start: Option<Snapshot>,
    pub link: String,
    pub error: String,
    restore_selection: bool,
}
impl Draft {
    pub fn new(runs: &[TextRun]) -> Self {
        let atoms = runs
            .iter()
            .flat_map(|r| {
                r.lang.text().chars().map(|ch| Atom {
                    ch,
                    myth: r.myth.clone(),
                })
            })
            .collect();
        let mut d = Self {
            atoms,
            text: String::new(),
            selection: 0..0,
            typing: None,
            undo: vec![],
            redo: vec![],
            composition_start: None,
            link: String::new(),
            error: String::new(),
            restore_selection: false,
        };
        d.rebuild();
        d
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn runs(&self) -> Vec<TextRun> {
        let mut out: Vec<TextRun> = vec![];
        for a in &self.atoms {
            if let Some(r) = out.last_mut().filter(|r| r.myth == a.myth) {
                let Language::En { txt } = &mut r.lang;
                txt.push(a.ch);
            } else {
                out.push(TextRun {
                    myth: a.myth.clone(),
                    lang: Language::En {
                        txt: a.ch.to_string(),
                    },
                });
            }
        }
        out
    }
    pub fn wire(&self) -> String {
        serde_json::to_string(&self.runs()).unwrap()
    }
    fn rebuild(&mut self) {
        self.text = self.atoms.iter().map(|a| a.ch).collect();
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            atoms: self.atoms.clone(),
            selection: self.selection.clone(),
            typing: self.typing.clone(),
        }
    }
    fn restore(&mut self, s: Snapshot) {
        self.atoms = s.atoms;
        self.selection = s.selection;
        self.typing = s.typing;
        self.rebuild();
        self.restore_selection = true;
    }
    fn record(&mut self, old: Snapshot) {
        if old.atoms != self.atoms || old.typing != self.typing {
            self.undo.push(old);
            while self.undo.len() > 1
                && (self.undo.len() > 100
                    || self.undo.iter().map(|s| s.atoms.len()).sum::<usize>() > 65536)
            {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }
    pub fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(s);
        }
    }
    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(s);
        }
    }
    fn caret_style(&self, at: usize) -> RunDescriptor {
        self.typing.clone().unwrap_or_else(|| {
            self.atoms
                .get(at.saturating_sub(1))
                .or_else(|| self.atoms.get(at))
                .map(|a| a.myth.clone())
                .unwrap_or_default()
        })
    }
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let range = range.start.min(self.atoms.len())..range.end.min(self.atoms.len());
        // Inherit the replaced selection's leading run, or the preceding run at a caret.
        let m = self.typing.clone().unwrap_or_else(|| {
            if !range.is_empty() {
                self.atoms[range.start].myth.clone()
            } else {
                self.caret_style(range.start)
            }
        });
        self.atoms.splice(
            range,
            text.chars().map(|ch| Atom {
                ch,
                myth: m.clone(),
            }),
        );
        self.rebuild();
    }
    pub fn format(
        &mut self,
        slot: &str,
        value: Option<serde_json::Value>,
        contract: &Editing,
    ) -> Result<()> {
        let old = self.snapshot();
        let change = |m: &mut RunDescriptor| {
            if let Some(v) = &value {
                m.insert(slot.into(), v.clone());
            } else {
                m.remove(slot);
            }
        };
        if self.selection.is_empty() {
            let mut m = self.caret_style(self.selection.start);
            change(&mut m);
            ensure!(
                contract.style(&m).is_some(),
                "Font/modifier combination unavailable"
            );
            self.typing = Some(m);
        } else {
            let mut next = self.atoms.clone();
            for a in &mut next[self.selection.clone()] {
                change(&mut a.myth);
                ensure!(
                    contract.style(&a.myth).is_some(),
                    "Font/modifier combination unavailable"
                );
            }
            self.atoms = next;
            self.typing = None;
        }
        self.record(old);
        Ok(())
    }
    pub fn toggle(&mut self, name: &str, contract: &Editing) -> Result<()> {
        let old = self.snapshot();
        let has = |m: &RunDescriptor| {
            m.get("/sys/modifiers")
                .and_then(|p| p["value"].as_array())
                .is_some_and(|v| v.contains(&json!(name)))
        };
        let remove = if self.selection.is_empty() {
            has(&self.caret_style(self.selection.start))
        } else {
            self.atoms[self.selection.clone()]
                .iter()
                .all(|a| has(&a.myth))
        };
        let transform = |m: &mut RunDescriptor| {
            let mut names = m
                .get("/sys/modifiers")
                .and_then(|p| p["value"].as_array())
                .cloned()
                .unwrap_or_default();
            names.retain(|v| v != name);
            if !remove {
                names.push(json!(name));
            }
            names.sort_by_key(|v| v.as_str().unwrap_or("").to_owned());
            if names.is_empty() {
                m.remove("/sys/modifiers");
            } else {
                m.insert(
                    "/sys/modifiers".into(),
                    json!({"kind":"modifiers","value":names}),
                );
            }
        };
        if self.selection.is_empty() {
            let mut m = self.caret_style(self.selection.start);
            transform(&mut m);
            ensure!(
                contract.style(&m).is_some(),
                "Font/modifier combination unavailable"
            );
            self.typing = Some(m);
        } else {
            let mut next = self.atoms.clone();
            for a in &mut next[self.selection.clone()] {
                transform(&mut a.myth);
                ensure!(
                    contract.style(&a.myth).is_some(),
                    "Font/modifier combination unavailable"
                );
            }
            self.atoms = next;
            self.typing = None;
        }
        self.record(old);
        Ok(())
    }
}
// A live shared document lets egui's layouter observe insertions during the same IME/input event.
struct Buffer {
    draft: Rc<RefCell<Draft>>,
    text: String,
}
impl TextBuffer for Buffer {
    fn is_mutable(&self) -> bool {
        true
    }
    fn as_str(&self) -> &str {
        &self.text
    }
    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        let mut d = self.draft.borrow_mut();
        d.replace(char_index..char_index, text);
        self.text = d.text.clone();
        text.chars().count()
    }
    fn delete_char_range(&mut self, r: Range<usize>) {
        let mut d = self.draft.borrow_mut();
        if !r.is_empty() && d.typing.is_none() {
            d.typing = d.atoms.get(r.start).map(|a| a.myth.clone());
        }
        d.replace(r, "");
        self.text = d.text.clone();
    }
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
}
pub fn widget(
    ui: &mut Ui,
    id: Id,
    rect: Rect,
    draft: &mut Draft,
    contract: &Editing,
    composing: bool,
    editable: bool,
) -> Response {
    let focused = ui.memory(|m| m.has_focus(id));
    // egui moves the caret on any pointer press, including a context-menu click.
    // Keep the selected range so menu commands apply to the intended text.
    let menu_selection = ui.input(|i| {
        (i.pointer.button_pressed(PointerButton::Secondary)
            && i.pointer.interact_pos().is_some_and(|p| rect.contains(p)))
        .then(|| draft.selection.clone())
    });
    let mut restore_cursor = std::mem::take(&mut draft.restore_selection);
    if focused && !composing && editable {
        for (key, name) in [(Key::B, "bold"), (Key::I, "italic"), (Key::U, "underline")] {
            if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, key)) {
                if let Err(e) = draft.toggle(name, contract) {
                    draft.error = e.to_string();
                }
                restore_cursor = true;
            }
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)) {
            draft.redo();
            restore_cursor = true;
        } else if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z)) {
            draft.undo();
            restore_cursor = true;
        }
    }
    if restore_cursor {
        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(draft.selection.start),
                egui::text::CCursor::new(draft.selection.end),
            )));
        state.clear_undoer();
        state.store(ui.ctx(), id);
    }
    let before = draft.snapshot();
    if composing && draft.composition_start.is_none() {
        draft.composition_start = Some(before.clone());
    }
    let shared = Rc::new(RefCell::new(draft.clone()));
    let mut buffer = Buffer {
        draft: shared.clone(),
        text: draft.text.clone(),
    };
    let mut layouter = |ui: &Ui, _: &dyn TextBuffer, width: f32| {
        let d = shared.borrow();
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = width;
        for run in d.runs() {
            let Some(s) = contract.style(&run.myth) else {
                continue;
            };
            let link = run.myth.contains_key("/sys/link");
            let paint = if link { &contract.link_style } else { s };
            let fmt = egui::TextFormat {
                font_id: crate::painter::font(&s.text_style),
                color: crate::painter::color(&paint.foreground),
                background: paint
                    .highlight
                    .as_ref()
                    .map(|h| crate::painter::color(&h.fill))
                    .unwrap_or(Color32::TRANSPARENT),
                underline: s
                    .underline
                    .as_ref()
                    .map(|u| Stroke::new(u.width as f32, crate::painter::color(&u.color)))
                    .unwrap_or(Stroke::NONE),
                line_height: Some(s.text_style.line_height as f32),
                ..Default::default()
            };
            job.append(run.lang.text(), 0., fmt);
        }
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let out = ui
        .scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(rect));
            ScrollArea::vertical()
                .id_salt(id.with("scroll"))
                .auto_shrink([false, false])
                .max_height(rect.height())
                .show(ui, |ui| {
                    TextEdit::multiline(&mut buffer)
                        .id(id)
                        .interactive(editable)
                        .char_limit(contract.max_bytes)
                        .frame(false)
                        .margin(Vec2::ZERO)
                        .desired_width(rect.width())
                        .layouter(&mut layouter)
                        .show(ui)
                })
                .inner
        })
        .inner;
    *draft = shared.borrow().clone();
    let mut state = out.state;
    if let Some(range) = menu_selection {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(range.start),
                egui::text::CCursor::new(range.end),
            )));
        draft.selection = range;
    } else if let Some(range) = out.cursor_range {
        let range = range.as_sorted_char_range();
        if range != draft.selection && !out.response.changed() {
            draft.typing = None;
        }
        draft.selection = range;
    }
    // Intermediate preedit strings are one transaction, including their metadata.
    if !composing {
        let before = draft.composition_start.take().unwrap_or(before);
        draft.record(before);
    }
    state.clear_undoer();
    state.store(ui.ctx(), id);
    out.response.context_menu(|ui| {
        ui.add_enabled_ui(editable && !composing, |ui| {
            for (name, label) in [
                ("bold", "Bold  ⌘B"),
                ("italic", "Italic  ⌘I"),
                ("underline", "Underline  ⌘U"),
            ] {
                if ui.button(label).clicked() {
                    if let Err(e) = draft.toggle(name, contract) {
                        draft.error = e.to_string();
                    }
                    ui.close();
                }
            }
            ui.menu_button("Font", |ui| {
                let mut paths = std::collections::BTreeSet::new();
                for s in &contract.styles {
                    let path = s
                        .myth
                        .get("/sys/font")
                        .and_then(|p| p["value"].as_str())
                        .unwrap_or("");
                    if paths.insert(path)
                        && ui
                            .button(if path.is_empty() {
                                "Default"
                            } else {
                                path.rsplit('/').next().unwrap()
                            })
                            .clicked()
                    {
                        let value = (!path.is_empty()).then(|| json!({"kind":"path","value":path}));
                        if let Err(e) = draft.format("/sys/font", value, contract) {
                            draft.error = e.to_string();
                        }
                        ui.close();
                    }
                }
            });
            ui.separator();
            ui.label("Link: /path or https://…");
            ui.text_edit_singleline(&mut draft.link);
            if ui.button("Apply link to selection").clicked() {
                let kind = if draft.link.starts_with('/') {
                    "path"
                } else {
                    "weblink"
                };
                let value = json!({"kind":kind,"value":draft.link});
                let mut test = RunDescriptor::new();
                test.insert("/sys/link".into(), value.clone());
                match crate::model::validate_run_myth(&test)
                    .and_then(|_| draft.format("/sys/link", Some(value), contract))
                {
                    Ok(()) => ui.close(),
                    Err(e) => draft.error = e.to_string(),
                }
            }
            if ui.button("Remove link").clicked() {
                let _ = draft.format("/sys/link", None, contract);
                ui.close();
            }
            ui.separator();
            if ui.button("Undo").clicked() {
                draft.undo();
                ui.close();
            }
            if ui.button("Redo").clicked() {
                draft.redo();
                ui.close();
            }
        });
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
    });
    out.response
}
