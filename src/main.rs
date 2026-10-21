use eframe::egui::{self, *};
use shrine_egui::inspector::Inspection;
use shrine_egui::{model::*, painter, session::*, transport::*};
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
use uuid::Uuid;

enum Event {
    Render {
        owner: Uuid,
        ticket: u64,
        result: Result<Box<Presentation>, String>,
    },
    Action {
        owner: Uuid,
        id: String,
        outcome: Outcome,
    },
    Deck {
        ticket: u64,
        result: Result<Change, String>,
    },
}
struct App {
    client: Client,
    session: Session,
    single: Option<Editor>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    guard: Option<Plan>,
    replacement: Option<(Uuid, u64)>,
    notice: String,
    deck_target: String,
    deck_view: String,
    deck_ticket: u64,
    width: f64,
    width_changed: Instant,
    quit_guard: bool,
    inspection: Option<Inspection>,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        painter::install(&cc.egui_ctx);
        shrine_egui::chrome::install(&cc.egui_ctx);
        cc.egui_ctx.set_zoom_factor(1.);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let server = std::env::var("GOO_SERVER").unwrap_or("http://127.0.0.1:8142".into());
        let client = Client::new(&server).expect("Valid GOO_SERVER");
        shrine_egui::images::set_server(&cc.egui_ctx, client.server());
        let (tx, rx) = mpsc::channel();
        let single = if std::env::args().any(|s| s == "--single") {
            Some(Editor::new(
                std::env::var("GOO_TARGET").unwrap_or("/0x11/app/ios_demo/note".into()),
                std::env::var("GOO_SLOT").unwrap_or("/sys/slot/content".into()),
            ))
        } else {
            None
        };
        let mut a = Self {
            client,
            session: Session::default(),
            single,
            tx,
            rx,
            guard: None,
            replacement: None,
            notice: server,
            deck_target: std::env::var("GOO_TARGET")
                .unwrap_or("/0x11/app/ios_demo/workspace".into()),
            deck_view: std::env::var("GOO_WORKSPACE")
                .unwrap_or("/0x11/gov/ios_demo/editors".into()),
            deck_ticket: 0,
            width: 700.,
            width_changed: Instant::now(),
            quit_guard: false,
            inspection: None,
        };
        if a.single.is_none() {
            a.load_deck(cc.egui_ctx.clone())
        }
        a
    }
    fn editor(&mut self, id: Uuid) -> Option<&mut Editor> {
        if self.single.as_ref().is_some_and(|e| e.id == id) {
            self.single.as_mut()
        } else {
            self.session.editor_mut(id)
        }
    }
    fn active(&mut self) -> Option<&mut Editor> {
        if self.single.is_some() {
            self.single.as_mut()
        } else {
            self.session.active_mut().and_then(|p| p.editor.as_mut())
        }
    }
    fn inspect(&mut self, node: Option<String>) {
        if let Some(e) = self.active() {
            match Inspection::capture(e, node) {
                Ok(capture) => self.inspection = Some(capture),
                Err(err) => self.notice = err.to_string(),
            }
        }
    }
    fn load_deck(&mut self, ctx: egui::Context) {
        self.deck_ticket += 1;
        self.session.intent += 1;
        self.guard = None;
        let ticket = self.deck_ticket;
        let client = self.client.clone();
        let target = self.deck_target.clone();
        let view = self.deck_view.clone();
        let tx = self.tx.clone();
        let width = self.width;
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Change> {
                let context = shrine_egui::model::Context::new(width);
                let p = client.render(&target, "view", &view, &context)?;
                p.validate_envelope(&context, &target)?;
                let tabs = shrine_egui::model::tabs(&p)?;
                let mut sources = BTreeMap::new();
                for t in &tabs {
                    if !sources.contains_key(&t.source) {
                        sources.insert(t.source.clone(), client.source(&t.source, &t.axis)?);
                    }
                }
                Ok(Change::Deck {
                    tabs,
                    sources,
                    basis: p.lifetime(),
                    publication: p.snapshot.publication,
                })
            })();
            let _ = tx.send(Event::Deck {
                ticket,
                result: result.map_err(|e| e.to_string()),
            });
            ctx.request_repaint();
        });
    }
    fn refresh(&mut self, id: Uuid, ctx: egui::Context) {
        let width = self.width;
        let Some(e) = self.editor(id) else { return };
        let context = shrine_egui::model::Context::new(width);
        let Some(ticket) = e.request(context.clone()) else {
            return;
        };
        let target = e.target.clone();
        let slot = e.slot.clone();
        let client = self.client.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = client
                .render(&target, "slot", &slot, &context)
                .map(Box::new)
                .map_err(|e| e.to_string());
            let _ = tx.send(Event::Render {
                owner: id,
                ticket,
                result,
            });
            ctx.request_repaint();
        });
    }
    fn send(&mut self, id: Uuid, i: Invocation, check: bool, ctx: egui::Context) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        if let Some(e) = self.editor(id) {
            e.sending = true;
        }
        std::thread::spawn(move || {
            let outcome = if check {
                client.status(&i)
            } else {
                client.activate(&i)
            };
            let _ = tx.send(Event::Action {
                owner: id,
                id: i.id().into(),
                outcome,
            });
            ctx.request_repaint();
        });
    }
    fn change(&mut self, c: Change) {
        self.deck_ticket += 1;
        match self.session.stage(c) {
            Ok(p) => {
                if self.session.needs_guard(&p) {
                    self.guard = Some(p);
                } else if let Err(e) = self.session.commit(p, false) {
                    self.notice = e.to_string();
                }
            }
            Err(e) => self.notice = e.to_string(),
        }
    }
    fn events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Render {
                    owner,
                    ticket,
                    result,
                } => {
                    if let Some(e) = self.editor(owner) {
                        if e.generation != ticket || e.suspended {
                            continue;
                        }
                        match result {
                            Ok(p) => {
                                if p.controls.iter().any(|c| c.r#become) {
                                    e.status = "Recipe-changing views require Explorer mode".into();
                                    continue;
                                }
                                if let Err(err) = e.receive(ticket, *p) {
                                    e.status = format!("Unsupported/invalid preparation: {err}");
                                }
                            }
                            Err(err) => {
                                e.status = format!("Refresh failed; old actions paused: {err}")
                            }
                        }
                    }
                }
                Event::Action { owner, id, outcome } => {
                    if let Some(e) = self.editor(owner)
                        && e.complete(&id, outcome)
                        && !e.suspended
                    {
                        self.refresh(owner, ctx.clone());
                    }
                }
                Event::Deck { ticket, result } => {
                    if ticket != self.deck_ticket {
                        continue;
                    }
                    match result {
                        Ok(c) => self.change(c),
                        Err(e) => self.notice = format!("Deck rejected; prior state retained: {e}"),
                    }
                }
            }
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.events(ctx);
        // IME preedit prevents mounting/replacing/hiding the focused owner.
        let composition = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Ime(ImeEvent::Preedit(s)) => Some(!s.is_empty()),
                    egui::Event::Ime(ImeEvent::Commit(_) | ImeEvent::Disabled) => Some(false),
                    _ => None,
                })
                .next_back()
        });
        if let Some(composing) = composition
            && let Some(e) = self.active()
        {
            e.composing = composing;
            if !composing && let Some(p) = e.candidate.take() {
                let ticket = e.generation;
                if let Err(err) = e.receive(ticket, p) {
                    e.status = err.to_string();
                }
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            let work = self
                .single
                .as_ref()
                .is_some_and(|e| e.dirty() || e.pending.is_some())
                || self
                    .session
                    .panes
                    .iter()
                    .filter_map(|p| p.editor.as_ref())
                    .any(|e| e.dirty() || e.pending.is_some());
            if work {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.quit_guard = true;
            }
        }
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Goo");
                ui.label("Prepared by Reaver · native egui");
                if ui.button("Inspect Goo").on_hover_text("Browse received nodes, bindings, and the original response. You can also right-click a component.").clicked() {
                    self.inspect(None);
                }
                shrine_egui::inspector::pipeline_toggle(ui);
                if ui.button("Refresh view").clicked()
                    && let Some(id) = self.active().map(|e| e.id)
                {
                    self.refresh(id, ctx.clone());
                }
                if self.single.is_none() && ui.button("Refresh declared tabs").clicked() {
                    self.load_deck(ctx.clone())
                }
            });
            ui.small(&self.notice);
            if self.single.is_none() {
                ui.horizontal(|ui| {
                    let tabs: Vec<_> = self
                        .session
                        .panes
                        .iter()
                        .map(|p| {
                            (
                                p.id,
                                p.tab.title.clone(),
                                p.editor.as_ref().is_some_and(|e| e.dirty()),
                            )
                        })
                        .collect();
                    for (id, title, dirty) in tabs {
                        if ui
                            .selectable_label(
                                self.session.active == Some(id),
                                format!("{title}{}", if dirty { " *" } else { "" }),
                            )
                            .clicked()
                            && !self.session.switch(id)
                        {
                            self.notice = "Finish IME composition before switching".into();
                        }
                    }
                    if ui.button("Reverse tabs").clicked() {
                        self.session.intent += 1;
                        self.guard = None;
                        self.session.panes.reverse();
                    }
                    if ui.button("Close tab").clicked()
                        && let Some(id) = self.session.active
                    {
                        self.change(Change::Remove { pane: id });
                    }
                });
            }
        });
        if self.single.is_none() {
            SidePanel::left("members")
                .exact_width(210.)
                .show(ctx, |ui| {
                    ui.heading("Selection");
                    let pane = self.session.active_mut().map(|p| {
                        (
                            p.id,
                            p.selected.as_ref().map(|m| m.identity()),
                            p.source.members.clone(),
                        )
                    });
                    if let Some((id, selected, members)) = pane {
                        for m in members {
                            let label = m.key.rsplit('/').next().unwrap().to_string();
                            if ui
                                .add_enabled(
                                    m.status == "available",
                                    Button::selectable(
                                        selected.as_ref() == Some(&m.identity()),
                                        format!("Note {label}"),
                                    ),
                                )
                                .clicked()
                            {
                                self.change(Change::Select {
                                    pane: id,
                                    member: m,
                                });
                            }
                        }
                        ui.small("Each tab owns its selection and draft.");
                    }
                });
        }
        CentralPanel::default().show(ctx, |ui| {
            let width = ui.available_width().floor().max(160.) as f64;
            if self.width != width {
                self.width = width;
                self.width_changed = Instant::now();
                if let Some(e) = self.active() {
                    e.stale = true;
                    e.generation += 1;
                    e.requested = None;
                }
            }
            let captures = self
                .session
                .active_mut()
                .map(|p| p.captures())
                .unwrap_or_default();
            let mut activation = None;
            let mut retry = None;
            let mut refresh = None;
            let mut replacement = None;
            let mut inspected = None;
            let ready = self.width_changed.elapsed() > Duration::from_millis(180);
            if let Some(e) = self.active() {
                ui.horizontal(|ui| {
                    ui.label(if e.dirty() {
                        "● Unsaved draft"
                    } else {
                        "○ No unsaved edits"
                    });
                    if e.sending {
                        ui.spinner();
                    }
                });
                ui.label(&e.status);
                if e.pending.is_some() && !e.sending {
                    ui.horizontal(|ui| {
                        if ui.button("Check save status").clicked() {
                            retry = Some((e.id, e.pending.clone().unwrap(), true));
                        }
                        if ui.button("Retry exact save").clicked() {
                            retry = Some((e.id, e.pending.clone().unwrap(), false));
                        }
                    });
                }
                if e.candidate.is_some() && !e.composing {
                    ui.horizontal(|ui| {
                        ui.label("Replacement would change this editor.");
                        if ui.button("Review replacement").clicked() {
                            replacement = Some((e.id, e.revision));
                        }
                    });
                }
                if ready && e.requested.is_none() && !e.suspended && e.pending.is_none() {
                    refresh = Some(e.id)
                }
                let id = e.id;
                ScrollArea::both().id_salt((id, "scroll")).show(ui, |ui| {
                    let (action, node) = painter::surface(ui, e, &captures);
                    activation = action.map(|c| (id, c));
                    inspected = node;
                });
            } else {
                ui.label("Choose a note to open its prepared editor.");
            }
            if replacement.is_some() {
                self.replacement = replacement;
            }
            if let Some(node) = inspected {
                self.inspect(Some(node));
            }
            if let Some(id) = refresh {
                self.refresh(id, ctx.clone())
            }
            if let Some((id, c)) = activation
                && let Some(e) = self.editor(id)
            {
                match e.activate(&c, &captures) {
                    Ok(i) => self.send(id, i, false, ctx.clone()),
                    Err(err) => self.notice = err.to_string(),
                }
            }
            if let Some((id, i, check)) = retry {
                self.send(id, i, check, ctx.clone())
            }
        });
        if let Some(plan) = self.guard.clone() {
            Window::new("Keep this work?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("This change affects an editor with unsaved or unresolved work.");
                    ui.label("Unknown saves and active composition must be resolved first.");
                    ui.horizontal(|ui| {
                        if ui.button("Stay").clicked() {
                            self.session.refuse();
                            self.guard = None;
                        }
                        if ui.button("Discard draft and continue").clicked() {
                            match self.session.commit(plan.clone(), true) {
                                Ok(()) => self.guard = None,
                                Err(e) => {
                                    self.notice = e.to_string();
                                    self.guard = None;
                                }
                            }
                        }
                    });
                });
        }
        if let Some((id, revision)) = self.replacement {
            Window::new("Replace prepared editor?").show(ctx, |ui| {
                ui.label("Discard working inputs only if this staged change is still current.");
                if ui.button("Stay").clicked() {
                    self.replacement = None;
                }
                if ui.button("Discard and replace").clicked() {
                    if let Some(e) = self.editor(id)
                        && let Err(err) = e.accept_candidate(revision)
                    {
                        self.notice = err.to_string();
                    }
                    self.replacement = None;
                }
            });
        }
        if self.quit_guard {
            Window::new("Work remains in this session").show(ctx,|ui|{ui.label("Save or discard drafts and resolve pending saves before closing. Recovery is in memory.");if ui.button("Keep working").clicked(){self.quit_guard=false;}});
        }
        if let Some(inspection) = &mut self.inspection {
            let mut open = true;
            inspection.show(ctx, &mut open);
            if !open {
                self.inspection = None;
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}
fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--explorer" || a == "--surfaces")
        || std::env::current_exe().ok().is_some_and(|p| {
            p.file_stem()
                .is_some_and(|n| n == "shrine-explorer" || n == "shrine-surfaces")
        })
    {
        return shrine_egui::explorer_app::run();
    }
    eframe::run_native(
        "Shrine Goo",
        eframe::NativeOptions {
            viewport: ViewportBuilder::default().with_inner_size([1000., 700.]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
