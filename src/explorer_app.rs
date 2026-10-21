use crate::{explorer::*, inspector::Inspection, model::*, painter, session::Editor, transport::*};
use anyhow::{Result, ensure};
use eframe::egui::{self, *};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone)]
struct Spec {
    id: String,
    node: Value,
    selection: Option<String>,
}
fn specs(nodes: &[Value]) -> Vec<Spec> {
    fn walk(nodes: &[Value], owner: &str, scopes: &[(String, String)], out: &mut Vec<Spec>) {
        for (index, n) in nodes.iter().enumerate() {
            let kind = node_text(n, "kind");
            let key = node_text(n, "key");
            let identity = if kind == "native" {
                format!("native:{}", node_text(n, "node"))
            } else if key.is_empty() {
                format!("{kind}:{index}")
            } else {
                format!("{kind}:{key}")
            };
            let id = format!("{owner}|{identity}");
            let selection = scopes
                .iter()
                .rev()
                .find(|(key, _)| key == node_text(n, "selection"))
                .map(|(_, id)| id.clone());
            out.push(Spec {
                id: id.clone(),
                node: n.clone(),
                selection,
            });
            let mut scopes = scopes.to_vec();
            if kind == "selection" {
                scopes.push((key.into(), id.clone()));
            }
            // Layout wrappers have no semantic ownership.
            walk(
                children(n),
                if ["split", "pane_stack"].contains(&kind) {
                    owner
                } else {
                    &id
                },
                &scopes,
                out,
            );
        }
    }
    let mut out = vec![];
    walk(nodes, "", &[], &mut out);
    out
}
#[derive(Clone)]
struct Selection {
    id: Uuid,
    revision: u64,
    member: Option<Member>,
}
#[derive(Clone)]
struct Surface {
    editor: Editor,
    selector: String,
    value: String,
    native: Option<String>,
    patch: Option<crate::patch::Work>,
    bundle: Option<Bundle>,
    wanted: f64,
    text_scale: f64,
    direction: String,
    loading: bool,
}
impl Surface {
    fn mounted(target: &str, selector: &str, value: &str, mut p: Presentation) -> Result<Self> {
        let patch = if p.kind == "pane" {
            let work = crate::patch::Work::new(crate::patch::Packet::admit(p.clone())?)?;
            p.kind = "view".into();
            p.controls.clear();
            p.description = None;
            p.display = Some(crate::model::Display {
                version: 1,
                profile: p.profile.clone(),
                context: p.context.clone(),
                coordinates: "absolute-css-px".into(),
                width: p.context.width,
                height: 0.,
                nodes: vec![],
            });
            Some(work)
        } else {
            None
        };
        let text_scale = p.context.text_scale;
        let direction = p.context.direction.clone();
        let mut editor = Editor::new(target.into(), value.into());
        let ticket = editor.request(p.context.clone()).unwrap();
        editor.receive(ticket, p)?;
        Ok(Self {
            editor,
            selector: selector.into(),
            value: value.into(),
            native: None,
            patch,
            bundle: None,
            wanted: 720.,
            text_scale,
            direction,
            loading: false,
        })
    }
}
#[derive(Clone)]
struct State {
    workspace: Workspace,
    specs: Vec<Spec>,
    trees: BTreeMap<String, Tree>,
    selections: BTreeMap<String, Selection>,
    surfaces: BTreeMap<String, Surface>,
    choices: BTreeMap<String, Vec<(String, String)>>,
    operations: BTreeMap<String, String>,
}
impl State {
    fn owners(&self) -> impl Iterator<Item = &Editor> {
        self.surfaces.values().flat_map(|s| {
            std::iter::once(&s.editor).chain(s.patch.iter().flat_map(|p| p.editors.values()))
        })
    }
    fn captures(&self, spec: &Spec) -> BTreeMap<String, SelectionCapture> {
        self.specs
            .iter()
            .filter(|s| node_text(&s.node, "kind") == "selection" && spec.id.starts_with(&s.id))
            .filter_map(|s| {
                let selection = self.selections.get(&s.id)?;
                let m = selection.member.clone()?;
                Some((
                    node_text(&s.node, "key").into(),
                    SelectionCapture {
                        selection: selection.id.to_string(),
                        incarnation: 1,
                        revision: selection.revision,
                        member: m,
                    },
                ))
            })
            .collect()
    }
    fn load(client: &Client, workspace: Workspace, old: Option<&State>) -> Result<Self> {
        let spec = specs(&workspace.nodes);
        let mut identities = std::collections::BTreeSet::new();
        ensure!(
            spec.iter().all(|s| identities.insert(s.id.clone())),
            "Duplicate semantic owner identity"
        );
        let compatible =
            old.filter(|s| s.workspace.packet.lifetime() == workspace.packet.lifetime());
        let mut state = Self {
            workspace,
            specs: spec,
            trees: BTreeMap::new(),
            selections: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            choices: BTreeMap::new(),
            operations: old.map(|s| s.operations.clone()).unwrap_or_default(),
        };
        for s in state.specs.clone() {
            match node_text(&s.node, "kind") {
                "selection" => {
                    state.selections.insert(
                        s.id.clone(),
                        compatible
                            .and_then(|o| o.selections.get(&s.id))
                            .cloned()
                            .unwrap_or(Selection {
                                id: Uuid::new_v4(),
                                revision: 0,
                                member: None,
                            }),
                    );
                }
                "native" => {
                    let p = native(&state.workspace.packet, &s.node)?;
                    let mut surface =
                        Surface::mounted(&p.snapshot.subject.clone(), "view", &p.view.clone(), p)?;
                    surface.native = Some(node_text(&s.node, "node").into());
                    state.surfaces.insert(s.id, surface);
                }
                "topos" => {
                    let selection = state
                        .specs
                        .iter()
                        .find(|x| Some(&x.id) == s.selection.as_ref())
                        .ok_or_else(|| anyhow::anyhow!("Missing selection owner"))?;
                    let axis = node_text(&selection.node, "axis");
                    let axis = if axis.is_empty() { "z" } else { axis };
                    let tree = Tree::new(
                        node_text(&selection.node, "source").into(),
                        axis.into(),
                        node_text(&s.node, "slot").into(),
                    );
                    let page = client.tree_page(&tree, None, None)?;
                    let mut tree = tree.with_page(page, None, None)?;
                    if let Some(previous) = compatible
                        .and_then(|o| o.trees.get(&s.id))
                        .filter(|p| p.life == tree.life)
                    {
                        // Rebuild only previously materialized branches, always against the new epoch.
                        for parent in &previous.order {
                            if !previous.next.contains_key(parent) {
                                continue;
                            }
                            if parent != &tree.source
                                && tree.members.get(parent).is_some_and(|m| {
                                    previous.members.get(parent).is_some_and(|old| {
                                        old.member.identity() == m.member.identity()
                                    })
                                })
                            {
                                let p = client.tree_page(&tree, Some(parent), None)?;
                                tree = tree.with_page(p, Some(parent), None)?;
                            }
                            let wanted = previous
                                .members
                                .values()
                                .filter(|m| m.parent.as_ref() == Some(parent))
                                .count();
                            while tree
                                .members
                                .values()
                                .filter(|m| m.parent.as_ref() == Some(parent))
                                .count()
                                < wanted
                            {
                                let Some(after) = tree.next.get(parent).cloned().flatten() else {
                                    break;
                                };
                                let p = client.tree_page(&tree, Some(parent), Some(&after))?;
                                tree = tree.with_page(p, Some(parent), Some(&after))?;
                            }
                        }
                        tree.expanded = previous
                            .expanded
                            .intersection(&tree.members.keys().cloned().collect())
                            .cloned()
                            .collect();
                    }
                    state.trees.insert(s.id.clone(), tree);
                }
                _ => {}
            }
        }
        for s in state
            .specs
            .clone()
            .into_iter()
            .filter(|s| node_text(&s.node, "kind") == "topos")
        {
            state.seed_rows(&s.id)?;
            let selection = state
                .selections
                .get_mut(s.selection.as_ref().unwrap())
                .unwrap();
            if let Some(m) = &selection.member {
                if let Some(n) = state.trees[&s.id]
                    .members
                    .values()
                    .find(|n| n.member.identity() == m.identity() && n.member.status == "available")
                {
                    selection.member = Some(n.member.clone());
                } else {
                    selection.member = None;
                    selection.revision += 1;
                }
            }
        }
        for s in state
            .specs
            .clone()
            .into_iter()
            .filter(|s| node_text(&s.node, "kind") == "follow")
        {
            if let Some(member) = state
                .selections
                .get(s.selection.as_ref().unwrap())
                .and_then(|v| v.member.clone())
            {
                let (selector, value) = compatible
                    .and_then(|o| o.surfaces.get(&s.id))
                    .map(|v| (v.selector.as_str(), v.value.as_str()))
                    .unwrap_or(("slot", node_text(&s.node, "slot")));
                let p = client.render(
                    &member.subject,
                    selector,
                    value,
                    &crate::model::Context::new(720.),
                )?;
                state.surfaces.insert(
                    s.id.clone(),
                    Surface::mounted(&member.subject, selector, value, p)?,
                );
                if let Some(choices) = compatible.and_then(|o| o.choices.get(&s.id)) {
                    state.choices.insert(s.id, choices.clone());
                }
            }
        }
        Ok(state)
    }
    fn seed_rows(&mut self, id: &str) -> Result<()> {
        let tree = &self.trees[id];
        for (key, bundle) in &tree.bundles {
            let row = format!("{id}|row:{key}");
            let mut s = Surface::mounted(key, "slot", &tree.slot, bundle.at(240.).clone())?;
            s.bundle = Some(bundle.clone());
            self.surfaces.insert(row, s);
        }
        Ok(())
    }
    fn merge(&mut self, old: &Self) -> Result<()> {
        for (id, s) in &mut self.surfaces {
            if let Some(previous) = old.surfaces.get(id).filter(|v| {
                v.editor.presentation.as_ref().map(Presentation::lifetime)
                    == s.editor.presentation.as_ref().map(Presentation::lifetime)
            }) {
                if let (Some(patch), Some(previous)) = (&mut s.patch, &previous.patch) {
                    patch.merge(previous)?;
                }
                let p = s.editor.presentation.clone().unwrap();
                s.text_scale = previous.text_scale;
                s.direction = previous.direction.clone();
                s.editor = previous.editor.clone();
                s.loading = false;
                if s.editor.pending.is_none() {
                    s.editor.suspended = false;
                    let t = s.editor.request(p.context.clone()).unwrap();
                    let _ = s.editor.receive(t, p);
                }
            }
        }
        Ok(())
    }
    fn affected<'a>(&'a self, next: &State) -> impl Iterator<Item = &'a Editor> {
        self.surfaces
            .iter()
            .filter(move |(key, s)| {
                next.surfaces.get(*key).is_none_or(|n| {
                    n.editor.presentation.as_ref().map(Presentation::lifetime)
                        != s.editor.presentation.as_ref().map(Presentation::lifetime)
                        || match (&s.patch, &n.patch) {
                            (Some(a), Some(b)) => {
                                !a.packet.same_encounter(&b.packet)
                                    || a.editors.keys().any(|k| {
                                        !b.editors.contains_key(k)
                                            || !b.packet.compatible_body(&a.packet, k)
                                    })
                            }
                            (Some(_), None) => true,
                            _ => false,
                        }
                })
            })
            .flat_map(|(_, s)| {
                std::iter::once(&s.editor).chain(s.patch.iter().flat_map(|p| p.editors.values()))
            })
    }
}

enum Event {
    Stage(u64, Result<Box<State>, String>),
    Render(Uuid, u64, Result<Box<Presentation>, String>),
    Action(Uuid, String, Outcome),
    Operation(String, Result<(), String>),
}
struct Pending {
    state: State,
    guard: Guard,
}
pub struct ExplorerApp {
    client: Client,
    endpoint: String,
    address: String,
    state: Option<State>,
    intent: u64,
    pending: Option<Pending>,
    stage_basis: Option<Vec<(Uuid, u64)>>,
    refresh_source: bool,
    was_focused: bool,
    busy: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    notice: String,
    inspection: Option<Inspection>,
    raw: Option<(String, String)>,
    operation: Option<String>,
    quit: bool,
}
impl ExplorerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let surfaces = std::env::args().any(|a| a == "--surfaces")
            || std::env::current_exe()
                .ok()
                .is_some_and(|p| p.file_stem().is_some_and(|n| n == "shrine-surfaces"));
        painter::install(&cc.egui_ctx);
        crate::chrome::install(&cc.egui_ctx);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            client: Client::new(
                &std::env::var("GOO_SERVER").unwrap_or(
                    if surfaces {
                        "http://127.0.0.1:8172"
                    } else {
                        "http://127.0.0.1:8164"
                    }
                    .into(),
                ),
            )
            .unwrap(),
            endpoint: std::env::var("GOO_ENDPOINT").unwrap_or("/goo-inspector".into()),
            address: std::env::var("GOO_PATH").unwrap_or(
                if surfaces {
                    "/0x11/app/patch_demo"
                } else {
                    "/0x11/app/ios_demo"
                }
                .into(),
            ),
            state: None,
            intent: 0,
            pending: None,
            stage_basis: None,
            refresh_source: false,
            was_focused: true,
            busy: false,
            tx,
            rx,
            notice: "Opening Explorer".into(),
            inspection: None,
            raw: None,
            operation: None,
            quit: false,
        };
        crate::images::set_server(&cc.egui_ctx, app.client.server());
        app.load(true, cc.egui_ctx.clone());
        app
    }
    fn launch(
        &mut self,
        ctx: egui::Context,
        job: impl FnOnce(Client, Option<State>) -> Result<State> + Send + 'static,
    ) {
        self.intent += 1;
        self.pending = None;
        self.busy = true;
        self.stage_basis = Some(
            self.state
                .iter()
                .flat_map(|s| s.owners().map(|e| (e.id, e.revision)))
                .collect(),
        );
        let ticket = self.intent;
        let client = self.client.clone();
        let old = self.state.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = job(client, old).map(Box::new).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::Stage(ticket, result));
            ctx.request_repaint();
        });
    }
    fn load(&mut self, navigate: bool, ctx: egui::Context) {
        let endpoint = self.endpoint.clone();
        let address = self.address.clone();
        self.launch(ctx, move |client, old| {
            let entry = match old.as_ref().filter(|_| !navigate) {
                Some(old) => old.workspace.entry.clone(),
                None => client.workspace_entry(&endpoint, &address)?,
            };
            let workspace = client.workspace(entry)?;
            State::load(&client, workspace, old.as_ref())
        });
    }
    fn select(&mut self, tree_id: String, key: String, ctx: egui::Context) {
        self.launch(ctx, move |client, old| {
            let mut state = old.ok_or_else(|| anyhow::anyhow!("No workspace"))?;
            let m = state.trees[&tree_id].members[&key].member.clone();
            ensure!(m.status == "available", "Unavailable member");
            let selection_id = state
                .specs
                .iter()
                .find(|s| s.id == tree_id)
                .unwrap()
                .selection
                .clone()
                .unwrap();
            let selection = state.selections.get_mut(&selection_id).unwrap();
            if selection
                .member
                .as_ref()
                .is_some_and(|v| v.identity() == m.identity())
            {
                return Ok(state);
            }
            selection.member = Some(m.clone());
            selection.revision += 1;
            // Discover matching ordinary recipes from the structured catalogue response.
            let catalogue = client.explorer_json(
                "/goo/render",
                true,
                vec![
                    ("target".into(), m.subject.clone()),
                    ("width".into(), "720".into()),
                    ("textScale".into(), "1".into()),
                    ("direction".into(), "ltr".into()),
                ],
            )?;
            let choices: Vec<_> = catalogue["views"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Missing view catalogue"))?
                .iter()
                .filter(|v| {
                    (v["kind"] == "view" || v["kind"] == "pane") && v.get("error").is_none()
                })
                .filter_map(|v| {
                    Some((
                        v["view"].as_str()?.to_owned(),
                        v["slot"].as_str().unwrap_or("").to_owned(),
                    ))
                })
                .collect();
            for spec in state.specs.clone().into_iter().filter(|s| {
                node_text(&s.node, "kind") == "follow"
                    && s.selection.as_ref() == Some(&selection_id)
            }) {
                let slot = node_text(&spec.node, "slot");
                let p =
                    client.render(&m.subject, "slot", slot, &crate::model::Context::new(720.))?;
                ensure!(
                    p.snapshot.life == m.life,
                    "Member changed while preparing selection; refresh tree"
                );
                state.surfaces.insert(
                    spec.id.clone(),
                    Surface::mounted(&m.subject, "slot", slot, p)?,
                );
                state.choices.insert(spec.id, choices.clone());
            }
            Ok(state)
        });
    }
    fn change_recipe(&mut self, owner: Uuid, slot: String, ctx: egui::Context) {
        self.launch(ctx, move |client, old| {
            let mut state = old.unwrap();
            let (key, surface) = state
                .surfaces
                .iter()
                .find(|(_, s)| s.editor.id == owner)
                .ok_or_else(|| anyhow::anyhow!("Originating recipe owner retired"))?;
            let key = key.clone();
            let target = surface.editor.target.clone();
            let p = client.render(
                &target,
                "slot",
                &slot,
                &crate::model::Context::new(surface.wanted.max(40.).floor()),
            )?;
            state
                .surfaces
                .insert(key, Surface::mounted(&target, "slot", &slot, p)?);
            Ok(state)
        });
    }
    fn recipe(&mut self, id: String, view: String, ctx: egui::Context) {
        self.launch(ctx, move |client, old| {
            let mut state = old.unwrap();
            ensure!(
                state
                    .choices
                    .get(&id)
                    .is_some_and(|c| c.iter().any(|(v, _)| *v == view)),
                "Recipe is not offered"
            );
            let target = state.surfaces[&id].editor.target.clone();
            let p = client.render(&target, "view", &view, &crate::model::Context::new(720.))?;
            state
                .surfaces
                .insert(id, Surface::mounted(&target, "view", &view, p)?);
            Ok(state)
        });
    }
    fn page(&mut self, id: String, parent: String, more: bool, ctx: egui::Context) {
        self.launch(ctx, move |client, old| {
            let mut s = old.unwrap();
            let tree = &s.trees[&id];
            let after = if more {
                tree.next[&parent].as_deref()
            } else {
                None
            };
            let page = client.tree_page(tree, Some(&parent), after)?;
            let next = tree.with_page(page, Some(&parent), after)?;
            s.trees.insert(id.clone(), next);
            s.seed_rows(&id)?;
            Ok(s)
        });
    }
    fn install(&mut self, mut next: State, guard: &Guard, discard: bool) -> Result<()> {
        if let Some(old) = &self.state {
            guard.check(self.intent, old.affected(&next), discard)?;
            next.merge(old)?;
        }
        if !self.was_focused {
            for s in next.surfaces.values_mut() {
                s.editor.suspend();
                if let Some(p) = &mut s.patch {
                    for e in p.editors.values_mut() {
                        e.suspend();
                    }
                }
            }
        }
        self.address = next.workspace.entry.source.clone();
        self.state = Some(next);
        self.notice = "Explorer ready".into();
        Ok(())
    }
    fn events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Stage(ticket, result) => {
                    if ticket != self.intent {
                        continue;
                    }
                    self.busy = false;
                    let current: Vec<_> = self
                        .state
                        .iter()
                        .flat_map(|s| s.owners().map(|e| (e.id, e.revision)))
                        .collect();
                    if self
                        .stage_basis
                        .take()
                        .is_some_and(|basis| basis != current)
                    {
                        self.notice = "Prepared change superseded by newer work; current workspace retained. Retry the change.".into();
                        continue;
                    }
                    match result {
                        Ok(next) => {
                            let guard = Guard::capture(
                                self.intent,
                                self.state.iter().flat_map(|s| s.affected(&next)),
                            );
                            if self.state.as_ref().is_some_and(|s| {
                                s.affected(&next).any(|e| e.dirty() || !e.can_leave())
                            }) {
                                self.pending = Some(Pending {
                                    state: *next,
                                    guard,
                                });
                                self.notice = "Change staged; current work retained".into();
                            } else if let Err(e) = self.install(*next, &guard, false) {
                                self.notice = e.to_string();
                            }
                        }
                        Err(error) => {
                            self.notice =
                                format!("Change refused; prior workspace retained: {error}")
                        }
                    }
                }
                Event::Render(id, ticket, result) => {
                    if let Some(s) = self.surface_mut(id) {
                        if s.editor.generation != ticket {
                            continue;
                        }
                        s.loading = false;
                        match result {
                            Ok(p) => {
                                if let Err(e) = s.editor.receive(ticket, *p) {
                                    s.editor.status = e.to_string();
                                }
                            }
                            Err(e) => {
                                s.editor.status = format!("Refresh failed; actions paused: {e}")
                            }
                        }
                    }
                }
                Event::Action(id, invocation, outcome) => {
                    let destination = self
                        .surface_mut(id)
                        .and_then(|s| s.editor.pending.as_ref())
                        .filter(|i| i.id() == invocation)
                        .and_then(|i| i.become_slot().map(str::to_owned));
                    let refresh = self.surface_mut(id).is_some_and(|s| {
                        if s.editor.complete(&invocation, outcome) {
                            s.loading = false;
                            s.bundle = None;
                            true
                        } else {
                            false
                        }
                    });
                    if refresh {
                        self.refresh_source = true;
                        if let Some(slot) = destination {
                            self.change_recipe(id, slot, ctx.clone());
                        } else {
                            self.refresh(id, ctx.clone());
                        }
                    }
                }
                Event::Operation(key, result) => {
                    if let Some(state) = self.state.as_mut() {
                        state.operations.insert(
                            key.clone(),
                            match &result {
                                Ok(()) => "Published".into(),
                                Err(e) => e.clone(),
                            },
                        );
                    }
                    match result {
                        Ok(()) => {
                            self.operation = None;
                            self.notice = format!("{key}: publication ready; refreshing");
                            self.load(false, ctx.clone());
                        }
                        Err(e) => self.notice = format!("{key}: {e}; installed workspace retained"),
                    }
                }
            }
        }
    }
    fn surface_mut(&mut self, id: Uuid) -> Option<&mut Surface> {
        self.state
            .as_mut()?
            .surfaces
            .values_mut()
            .find(|s| s.editor.id == id)
    }
    fn send(&mut self, id: Uuid, i: Invocation, check: bool, ctx: egui::Context) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        if let Some(s) = self.surface_mut(id) {
            s.editor.sending = true;
        }
        std::thread::spawn(move || {
            let outcome = if check {
                client.status(&i)
            } else {
                client.activate(&i)
            };
            let _ = tx.send(Event::Action(id, i.id().into(), outcome));
            ctx.request_repaint();
        });
    }
    fn refresh(&mut self, id: Uuid, ctx: egui::Context) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let workspace = self.state.as_ref().unwrap().workspace.entry.view.clone();
        let host = self.state.as_ref().unwrap().workspace.entry.host.clone();
        let Some(s) = self.surface_mut(id) else {
            return;
        };
        if let Some(patch) = &mut s.patch {
            patch.bind(&client);
            patch.refresh(&ctx);
            return;
        }
        if s.loading {
            return;
        }
        let width = s.wanted.max(40.).floor();
        if let Some(bundle) = &s.bundle {
            let p = bundle.at(width).clone();
            if width >= 240.
                && s.editor
                    .presentation
                    .as_ref()
                    .is_some_and(|old| old.snapshot.expected == p.snapshot.expected)
                && !s.editor.stale
            {
                if s.editor.requested.as_ref() != Some(&p.context) {
                    let t = s.editor.request(p.context.clone());
                    if let Some(t) = t {
                        let _ = s.editor.receive(t, p);
                    }
                }
                return;
            }
        }
        let render_context = crate::model::Context {
            width,
            text_scale: s.text_scale,
            direction: s.direction.clone(),
        };
        let Some(ticket) = s.editor.request(render_context.clone()) else {
            return;
        };
        s.loading = true;
        let target = s.editor.target.clone();
        let selector = s.selector.clone();
        let value = s.value.clone();
        let native = s.native.clone();
        std::thread::spawn(move || {
            let p = if let Some(node) = native {
                client
                    .prepared(
                        vec![
                            ("target".into(), target),
                            ("view".into(), value),
                            ("workspace".into(), workspace.clone()),
                            ("node".into(), node),
                        ],
                        width,
                    )
                    .and_then(|mut p| {
                        ensure!(
                            p.controls.iter().all(|c| !c.invoke),
                            "Workspace cannot invoke domain actions"
                        );
                        let actual: Host = serde_json::from_value(
                            p.host
                                .clone()
                                .ok_or_else(|| anyhow::anyhow!("Missing native host"))?,
                        )?;
                        ensure!(
                            actual == host && p.view == workspace,
                            "Native workspace host/recipe mismatch"
                        );
                        ensure!(p.kind == "native", "Expected native-node preparation");
                        p.kind = "view".into();
                        p.host = None;
                        Ok(p)
                    })
            } else {
                client.render(&target, &selector, &value, &render_context)
            };
            let _ = tx.send(Event::Render(
                id,
                ticket,
                p.map(Box::new).map_err(|e| e.to_string()),
            ));
            ctx.request_repaint();
        });
    }
    fn operate(&mut self, key: String, ctx: egui::Context, check: bool) {
        if self.operation.is_some() && !check {
            return;
        }
        if self
            .state
            .as_ref()
            .is_some_and(|s| s.owners().any(|e| e.dirty() || !e.can_leave()))
        {
            self.notice =
                "Save or discard drafts and resolve saves before publishing mount changes.".into();
            return;
        }
        let Some(op) = self
            .state
            .as_ref()
            .unwrap()
            .workspace
            .entry
            .host
            .operations
            .iter()
            .find(|o| o.key == key)
            .cloned()
        else {
            return;
        };
        self.operation = Some(key.clone());
        if let Some(state) = self.state.as_mut() {
            state.operations.insert(
                key.clone(),
                if check {
                    "Checking publication…"
                } else {
                    "Scanning and publishing…"
                }
                .into(),
            );
        }
        let client = self.client.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let call = |action: &str, scan: Option<&str>| -> Result<Value> {
                    let mut fields = vec![("source".into(), op.source.clone())];
                    if let Some(scan) = scan {
                        fields.push(("scan".into(), scan.into()));
                    }
                    let v = client.explorer_json(
                        &format!("/grove/dev/{action}"),
                        action != "status",
                        fields,
                    )?;
                    v["units"]
                        .as_array()
                        .and_then(|units| units.iter().find(|u| u["source"] == op.source))
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Mount source unavailable"))
                };
                if check {
                    let status = call("status", None)?;
                    ensure!(
                        status["completed"] == status["requested"],
                        "Publication still pending"
                    );
                    ensure!(
                        status["ready"] == true,
                        "Publication failed: {}",
                        status["diagnostic"]
                    );
                    return Ok(());
                }
                let old = call("status", None)?;
                let unit = call("rescan", old["requested"].as_str())?;
                let requested = unit["requested"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing scan receipt"))?
                    .to_owned();
                for _ in 0..1200 {
                    let u = call("status", None)?;
                    ensure!(
                        u["requested"] == requested,
                        "Scan superseded; check publication status"
                    );
                    if u["completed"]
                        .as_str()
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0)
                        >= requested.parse::<u64>()?
                    {
                        ensure!(
                            u["ready"] == true,
                            "Publication failed: {}",
                            u["diagnostic"]
                        );
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
                anyhow::bail!("Publication outcome unresolved; check mount status before retrying")
            })()
            .map_err(|e| e.to_string());
            let _ = tx.send(Event::Operation(key, result));
            ctx.request_repaint();
        });
    }
    fn surface_ui(
        ui: &mut Ui,
        id: &str,
        state: &mut State,
        commands: &mut Vec<Command>,
        captures: &BTreeMap<String, SelectionCapture>,
    ) {
        if let Some(s) = state.surfaces.get_mut(id) {
            if let Some(patch) = &mut s.patch {
                patch.ui(ui);
                s.editor.composing = patch.interacting();
                return;
            }
            if s.native.is_none() {
                ui.horizontal(|ui| {
                    let before = (s.text_scale, s.direction.clone());
                    ComboBox::from_id_salt((id, "scale"))
                        .selected_text(format!("Text {}×", s.text_scale))
                        .show_ui(ui, |ui| {
                            for value in [1., 1.5, 2.] {
                                ui.selectable_value(&mut s.text_scale, value, format!("{value}×"));
                            }
                        });
                    ComboBox::from_id_salt((id, "direction"))
                        .selected_text(&s.direction)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut s.direction, "ltr".into(), "LTR");
                            ui.selectable_value(&mut s.direction, "rtl".into(), "RTL");
                        });
                    if before != (s.text_scale, s.direction.clone()) {
                        s.editor.requested = None;
                        s.editor.stale = true;
                    }
                });
            }
            let e = &mut s.editor;
            s.wanted = ui.available_width() as f64;
            if s.native.is_none() {
                ui.horizontal(|ui| {
                    if e.dirty() {
                        ui.strong("Unsaved draft");
                    }
                    if ui.small_button("Inspect Goo").clicked() {
                        commands.push(Command::Inspect(e.id, None));
                    }
                    if ui.small_button("Refresh").clicked() {
                        commands.push(Command::Refresh(e.id));
                    }
                    if e.dirty() && e.can_leave() && ui.small_button("Discard draft").clicked() {
                        commands.push(Command::Discard(e.id, e.revision));
                    }
                });
            }
            if let Some(i) = e.pending.clone() {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!e.sending, Button::new("Check save status"))
                        .clicked()
                    {
                        commands.push(Command::Retry(e.id, i.clone(), true));
                    }
                    if ui
                        .add_enabled(!e.sending, Button::new("Retry exact save"))
                        .clicked()
                    {
                        commands.push(Command::Retry(e.id, i.clone(), false));
                    }
                });
            }
            if let Some(candidate) = e.candidate.as_ref() {
                ui.label(format!(
                    "Replacement staged for {}; draft retained",
                    candidate.view
                ));
                if ui.button("Discard and accept replacement").clicked() {
                    commands.push(Command::Candidate(e.id, e.revision));
                }
            }
            if s.native.is_none() || e.status.contains("failed") {
                ui.small(&e.status);
            }
            let (action, node) = painter::surface(ui, e, captures);
            if let Some(node) = node {
                commands.push(Command::Inspect(e.id, Some(node)));
            }
            if let Some(c) = action {
                commands.push(Command::Action(e.id, c, captures.clone()));
            }
            if e.pending.is_none()
                && !s.loading
                && !e.suspended
                && e.candidate.is_none()
                && (e.requested.is_none()
                    || e.stale && e.status == "Save accepted; reread required"
                    || e.requested
                        .as_ref()
                        .is_some_and(|c| (c.width - s.wanted.floor()).abs() > 1.))
            {
                commands.push(Command::Refresh(e.id));
            }
        }
    }
    fn tree_ui(ui: &mut Ui, spec: &Spec, state: &mut State, commands: &mut Vec<Command>) {
        let camera_id = Id::new((
            state.selections[spec.selection.as_ref().unwrap()].id,
            "diagram-camera",
        ));
        let mut camera = ui
            .ctx()
            .data_mut(|d| d.get_temp::<crate::diagram::Camera>(camera_id))
            .unwrap_or(crate::diagram::Camera {
                diagram: node_text(&spec.node, "renderer") == "diagram_tree",
                scale: 1.,
            });
        let mut fit = false;
        let details_id = Id::new((&spec.id, "tree-details"));
        let mut details = ui
            .ctx()
            .data_mut(|d| d.get_temp::<bool>(details_id))
            .unwrap_or(false);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut camera.diagram, false, "Outline");
            ui.selectable_value(&mut camera.diagram, true, "Diagram");
            if !camera.diagram {
                ui.checkbox(&mut details, "Details")
                    .on_hover_text("Show Goo's prepared row presentations");
            }
            if camera.diagram {
                ui.add(Slider::new(&mut camera.scale, 0.25..=2.).text("Zoom"));
                if ui.button("Fit").clicked() {
                    fit = true;
                }
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(details_id, details));
        ui.ctx()
            .data_mut(|d| d.insert_temp(camera_id, camera.clone()));
        if camera.diagram {
            Self::diagram_ui(ui, spec, state, commands, &mut camera.scale, fit);
            ui.ctx().data_mut(|d| d.insert_temp(camera_id, camera));
            return;
        }
        let original = &state.trees[&spec.id];
        let mut tree = Tree::new(
            original.source.clone(),
            original.axis.clone(),
            original.slot.clone(),
        );
        tree.members = original.members.clone();
        tree.order = original.order.clone();
        tree.next = original.next.clone();
        tree.expanded = original.expanded.clone();
        tree.omitted = original.omitted.clone();
        let selected = state.selections[spec.selection.as_ref().unwrap()]
            .member
            .as_ref()
            .map(|m| m.key.clone());
        fn row(
            ui: &mut Ui,
            key: &str,
            tree: &Tree,
            spec: &Spec,
            selected: &Option<String>,
            state: &mut State,
            commands: &mut Vec<Command>,
        ) {
            let n = &tree.members[key];
            let open = tree.expanded.contains(key);
            let details = ui
                .ctx()
                .data_mut(|d| d.get_temp::<bool>(Id::new((&spec.id, "tree-details"))))
                .unwrap_or(false);
            ui.push_id((&spec.id, key), |ui| {
                let id = format!("{}|row:{key}", spec.id);
                let captures = state.captures(spec);
                if let Some(s) = state.surfaces.get_mut(&id) {
                    s.wanted = (ui.available_width() - 32.).max(40.) as f64;
                    if let Some(bundle) = &s.bundle {
                        let p = bundle.at(s.wanted).clone();
                        if s.editor
                            .presentation
                            .as_ref()
                            .is_some_and(|v| v.context != p.context)
                            && !s.editor.stale
                            && !s.editor.dirty()
                            && s.editor.pending.is_none()
                        {
                            let ticket =
                                s.editor.request(p.context.clone()).unwrap();
                            let _ = s.editor.receive(ticket, p);
                        }
                    }
                    let needs_width = s.wanted < 240.
                        && s.editor.requested.as_ref().is_some_and(|c| {
                            (c.width - s.wanted.max(40.).floor()).abs() > 1.
                        });
                    if !s.loading
                        && !s.editor.suspended
                        && s.editor.pending.is_none()
                        && (s.editor.requested.is_none() || needs_width)
                    {
                        commands.push(Command::Refresh(s.editor.id));
                    }
                }

                let height = if details { state.surfaces.get(&id)
                    .and_then(|s| s.editor.presentation.as_ref())
                    .and_then(|p| p.display.as_ref())
                    .map(|d| d.height as f32 + 8.)
                    .unwrap_or(28.) } else { 28. };
                let mut child_action = false;
                let r = crate::chrome::tree_row(
                    ui, key, selected.as_deref() == Some(key),
                    n.member.status == "available", height, |ui| {
                        ui.spacing_mut().item_spacing.x = 4.;
                        ui.spacing_mut().interact_size.y = 20.;
                        ui.horizontal(|ui| {
                            if n.has_children {
                                if crate::chrome::disclosure(ui, open).clicked() {
                                    commands.push(Command::Expand(spec.id.clone(), key.into()));
                                    child_action = true;
                                }
                            } else {
                                ui.allocate_space(vec2(20., 20.));
                            }
                            if details && n.member.status == "available" {
                                if let Some(s) = state.surfaces.get_mut(&id) {
                                    let (action, node) = painter::surface(ui, &mut s.editor, &captures);
                                    if let Some(node) = node {
                                        commands.push(Command::Inspect(s.editor.id, Some(node)));
                                        child_action = true;
                                    }
                                    if let Some(c) = action {
                                        commands.push(Command::Action(s.editor.id, c, captures));
                                        child_action = true;
                                    }
                                }
                            } else {
                                ui.add(Label::new(key.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or("Root"))
                                    .selectable(false));
                            }
                        });
                    }).response;
                if r.clicked() && !child_action {
                    commands.push(Command::Select(spec.id.clone(), key.into()));
                }
                r.context_menu(|ui| {
                    if let Some(s) = state.surfaces.get(&id)
                        && ui.button("Inspect row Goo").clicked() {
                        commands.push(Command::Inspect(s.editor.id, None));
                        ui.close();
                    }
                    if ui.button("Inspect tree contract").clicked() {
                        commands.push(Command::Raw(
                            "Tree".into(),
                            serde_json::to_string_pretty(
                                &json!({"member":n,"bundle":state.trees[&spec.id].bundles.get(key)}),
                            )
                            .unwrap(),
                        ));
                        ui.close();
                    }
                    if ui.button("Open subtree").clicked() {
                        commands.push(Command::Navigate(key.into()));
                        ui.close();
                    }
                });
                if open {
                    ui.indent(key, |ui| {
                        for child in &tree.order {
                            if tree.members[child].parent.as_deref() == Some(key) {
                                row(ui, child, tree, spec, selected, state, commands);
                            }
                        }
                        if tree.next.get(key).is_some_and(|v| v.is_some())
                            && ui.button("Load more children").clicked()
                        {
                            commands.push(Command::More(spec.id.clone(), key.into()));
                        }
                        if let Some(omitted) = tree.omitted.get(key).filter(|v| **v > 0) {
                            ui.label(format!("{omitted} unrepresentable keys omitted"));
                        }
                    });
                }
            });
        }
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            row(ui, &tree.source, &tree, spec, &selected, state, commands);
        });
    }
    fn diagram_ui(
        ui: &mut Ui,
        spec: &Spec,
        state: &mut State,
        commands: &mut Vec<Command>,
        scale: &mut f32,
        fit: bool,
    ) {
        let tree = &state.trees[&spec.id];
        let mut items = vec![];
        for key in &tree.order {
            let n = &tree.members[key];
            let mut parent = n.parent.as_deref();
            let mut visible = true;
            while let Some(p) = parent {
                if !tree.expanded.contains(p) {
                    visible = false;
                    break;
                }
                parent = tree.members.get(p).and_then(|n| n.parent.as_deref());
            }
            if !visible {
                continue;
            }
            let height = state
                .surfaces
                .get(&format!("{}|row:{key}", spec.id))
                .and_then(|s| s.editor.presentation.as_ref())
                .and_then(|p| p.display.as_ref())
                .map(|d| d.height as f32)
                .unwrap_or(36.);
            let width = state
                .surfaces
                .get(&format!("{}|row:{key}", spec.id))
                .and_then(|s| s.editor.presentation.as_ref())
                .map(|p| p.context.width as f32)
                .unwrap_or(240.);
            items.push(crate::diagram::Item {
                key: key.clone(),
                parent: n.parent.clone(),
                size: vec2(width, height + 66.),
            });
        }
        let boxes = match crate::diagram::arrange(&items) {
            Ok(v) => v,
            Err(e) => {
                ui.label(e.to_string());
                return;
            }
        };
        let extent = boxes.values().fold(Rect::NOTHING, |a, b| a.union(*b));
        if fit {
            *scale = ((ui.available_width() - 24.) / extent.max.x.max(1.))
                .min(650. / extent.max.y.max(1.))
                .clamp(0.25, 2.);
        }
        let scale = *scale;
        let members = tree.members.clone();
        let expanded = tree.expanded.clone();
        let next = tree.next.clone();
        let selected = state.selections[spec.selection.as_ref().unwrap()]
            .member
            .as_ref()
            .map(|m| m.key.clone());
        let captures = state.captures(spec);
        let (canvas, _) = ui.allocate_exact_size(extent.max.to_vec2() * scale, Sense::hover());
        let layer = LayerId::new(ui.layer_id().order, Id::new((&spec.id, "diagram-layer")));
        let transform = egui::emath::TSTransform::new(canvas.min.to_vec2(), scale);
        ui.ctx().set_transform_layer(layer, transform);
        ui.ctx().set_sublayer(ui.layer_id(), layer);
        let mut world = ui.new_child(UiBuilder::new().layer_id(layer).max_rect(extent));
        world.set_clip_rect(transform.inverse() * ui.clip_rect());
        for item in &items {
            let r = boxes[&item.key];
            if let Some(parent) = &item.parent {
                let from = boxes[parent].right_center();
                let to = r.left_center();
                let mid = (from.x + to.x) / 2.;
                world
                    .painter()
                    .add(egui::epaint::CubicBezierShape::from_points_stroke(
                        [from, pos2(mid, from.y), pos2(mid, to.y), to],
                        false,
                        Color32::TRANSPARENT,
                        Stroke::new(1_f32, ui.visuals().weak_text_color()),
                    ));
            }
            world.scope_builder(UiBuilder::new().max_rect(r), |ui| {
                let n = &members[&item.key];
                ui.horizontal(|ui| {
                    if n.has_children
                        && ui
                            .small_button(if expanded.contains(&item.key) {
                                "−"
                            } else {
                                "+"
                            })
                            .clicked()
                    {
                        commands.push(Command::Expand(spec.id.clone(), item.key.clone()));
                    }
                    if n.member.status == "available"
                        && ui
                            .selectable_label(
                                selected.as_ref() == Some(&item.key),
                                format!("Select {}", item.key.rsplit('/').next().unwrap_or("/")),
                            )
                            .clicked()
                    {
                        commands.push(Command::Select(spec.id.clone(), item.key.clone()));
                    }
                });
                if let Some(s) = state
                    .surfaces
                    .get_mut(&format!("{}|row:{}", spec.id, item.key))
                {
                    if s.editor.stale
                        && s.editor.pending.is_none()
                        && !s.loading
                        && !s.editor.suspended
                    {
                        commands.push(Command::Refresh(s.editor.id));
                    }
                    let (action, node) = painter::surface(ui, &mut s.editor, &captures);
                    if let Some(c) = action {
                        commands.push(Command::Action(s.editor.id, c, captures.clone()));
                    }
                    if node.is_some() {
                        commands.push(Command::Inspect(s.editor.id, node));
                    }
                } else {
                    ui.label(&item.key);
                }
                if next.get(&item.key).is_some_and(|v| v.is_some())
                    && ui.small_button("More children").clicked()
                {
                    commands.push(Command::More(spec.id.clone(), item.key.clone()));
                }
            });
        }
    }

    fn nodes_ui(
        ui: &mut Ui,
        nodes: &[Value],
        owner: &str,
        state: &mut State,
        commands: &mut Vec<Command>,
        address: &mut String,
    ) {
        // Group adjacent host controls without changing declaration identities.
        let mut index = 0;
        while index < nodes.len() {
            let toolbar = |n: &Value| {
                matches!(
                    node_text(n, "kind"),
                    "navigate" | "operation" | "operation_status"
                )
            };
            if toolbar(&nodes[index]) {
                ui.horizontal_wrapped(|ui| {
                    while index < nodes.len() && toolbar(&nodes[index]) {
                        Self::node_ui(ui, &nodes[index], index, owner, state, commands, address);
                        index += 1;
                    }
                });
                ui.add_space(8.);
            } else {
                Self::node_ui(ui, &nodes[index], index, owner, state, commands, address);
                index += 1;
            }
        }
    }
    fn node_ui(
        ui: &mut Ui,
        n: &Value,
        index: usize,
        owner: &str,
        state: &mut State,
        commands: &mut Vec<Command>,
        address: &mut String,
    ) {
        let kind = node_text(n, "kind");
        let key = node_text(n, "key");
        let identity = if kind == "native" {
            format!("native:{}", node_text(n, "node"))
        } else if key.is_empty() {
            format!("{kind}:{index}")
        } else {
            format!("{kind}:{key}")
        };
        let id = format!("{owner}|{identity}");
        let spec = state.specs.iter().find(|s| s.id == id).cloned().unwrap();
        ui.push_id(&id, |ui| match kind {
            "native" => Self::surface_ui(ui, &id, state, commands, &BTreeMap::new()),
            "navigate" => {
                ui.horizontal(|ui| {
                    let label = node_text(n, "label");
                    if node_text(n, "mode") == "input" {
                        let r = ui.add(
                            TextEdit::singleline(address)
                                .desired_width(360_f32.min((ui.available_width() - 130.).max(80.)))
                                .margin(vec2(8., 6.))
                                .font(egui::TextStyle::Monospace),
                        );
                        if ui.button(label).clicked()
                            || r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
                        {
                            commands.push(Command::Navigate(address.clone()));
                        }
                    } else if ui.button(label).clicked() {
                        let source = node_text(n, "source");
                        let parent = source
                            .rsplit_once('/')
                            .map(|(v, _)| if v.is_empty() { "/" } else { v })
                            .unwrap_or("/");
                        commands.push(Command::Navigate(parent.into()));
                    }
                });
            }
            "operation" => {
                if ui.button(node_text(n, "label")).clicked() {
                    commands.push(Command::Operate(node_text(n, "operation").into()));
                }
            }
            "operation_status" => {
                ui.small(format!(
                    "{}: {}",
                    node_text(n, "label"),
                    state
                        .operations
                        .get(node_text(n, "operation"))
                        .map(String::as_str)
                        .unwrap_or("Idle")
                ));
            }
            "topos" => Self::tree_ui(ui, &spec, state, commands),
            "follow" => {
                if let Some(surface) = state.surfaces.get(&id) {
                    let current = surface
                        .editor
                        .presentation
                        .as_ref()
                        .map(|p| p.view.clone())
                        .unwrap_or_default();
                    let choices = state.choices.get(&id).cloned().unwrap_or_default();
                    ComboBox::from_id_salt("recipe")
                        .selected_text(format!(
                            "Recipe  ·  {}",
                            current.rsplit('/').next().unwrap_or(&current)
                        ))
                        .width(ui.available_width().min(320.))
                        .show_ui(ui, |ui| {
                            for (view, slot) in choices {
                                if ui
                                    .selectable_label(view == current, format!("{slot} · {view}"))
                                    .clicked()
                                    && view != current
                                {
                                    commands.push(Command::Recipe(id.clone(), view));
                                    ui.close();
                                }
                            }
                        })
                        .response
                        .on_hover_text(&current);
                    ui.add_space(4.);
                    let captures = state.captures(&spec);
                    Self::surface_ui(ui, &id, state, commands, &captures);
                } else {
                    ui.label("Select a record to inspect its values.");
                }
            }
            "split" => {
                let nodes = children(n);
                // A tree/follower split is native navigation chrome; give the
                // prepared content most of the workspace. Other splits stay equal.
                fn contains_tree(node: &Value) -> bool {
                    node_text(node, "kind") == "topos" || children(node).iter().any(contains_tree)
                }
                if nodes.len() == 2 && contains_tree(&nodes[0]) {
                    let width = ui.available_width();
                    let height = ui.available_height().max(0.);
                    if width < 760. {
                        let gap = ui.spacing().item_spacing.y;
                        let sidebar_height = ((height - gap).max(0.) * 0.35).min(240.);
                        for (node, slot_height) in nodes
                            .iter()
                            .zip([sidebar_height, (height - gap - sidebar_height).max(0.)])
                        {
                            ui.allocate_ui_with_layout(
                                vec2(width, slot_height),
                                egui::Layout::top_down(Align::Min),
                                |ui| {
                                    Self::nodes_ui(
                                        ui,
                                        std::slice::from_ref(node),
                                        owner,
                                        state,
                                        commands,
                                        address,
                                    )
                                },
                            );
                        }
                    } else {
                        ui.horizontal_top(|ui| {
                            let sidebar = (width * 0.27).clamp(260., 360.);
                            ui.allocate_ui_with_layout(
                                vec2(sidebar, height),
                                egui::Layout::top_down(Align::Min),
                                |ui| {
                                    Self::nodes_ui(
                                        ui,
                                        &nodes[..1],
                                        owner,
                                        state,
                                        commands,
                                        address,
                                    );
                                },
                            );
                            ui.allocate_ui_with_layout(
                                vec2((width - sidebar - 8.).max(0.), height),
                                egui::Layout::top_down(Align::Min),
                                |ui| {
                                    Self::nodes_ui(
                                        ui,
                                        &nodes[1..],
                                        owner,
                                        state,
                                        commands,
                                        address,
                                    );
                                },
                            );
                        });
                    }
                } else if !nodes.is_empty() {
                    ui.columns(nodes.len(), |columns| {
                        for (column, node) in columns.iter_mut().zip(nodes) {
                            Self::nodes_ui(
                                column,
                                std::slice::from_ref(node),
                                owner,
                                state,
                                commands,
                                address,
                            );
                        }
                    });
                }
            }
            "pane" => {
                if !key.is_empty() && !owner.is_empty() {
                    crate::chrome::scroll_pane(
                        ui,
                        &id,
                        key.rsplit('/').next().unwrap_or(key),
                        |ui| {
                            Self::nodes_ui(ui, children(n), &id, state, commands, address);
                        },
                    );
                } else {
                    Self::nodes_ui(ui, children(n), &id, state, commands, address);
                }
            }
            "selection" => Self::nodes_ui(ui, children(n), &id, state, commands, address),
            "or" => {
                let alternatives = children(n);
                if let Some(first) = alternatives.first() {
                    let follow = state
                        .specs
                        .iter()
                        .find(|s| s.id.starts_with(&id) && node_text(&s.node, "kind") == "follow");
                    let selected = follow.is_some_and(|s| state.surfaces.contains_key(&s.id));
                    if selected {
                        Self::nodes_ui(
                            ui,
                            std::slice::from_ref(first),
                            &id,
                            state,
                            commands,
                            address,
                        );
                    } else if alternatives.len() > 1 {
                        Self::nodes_ui(ui, &alternatives[1..], &id, state, commands, address);
                    }
                }
            }
            "pane_stack" => Self::nodes_ui(ui, children(n), owner, state, commands, address),
            _ => {
                ui.colored_label(Color32::RED, format!("Unsupported component {kind}"));
            }
        });
    }
}
enum Command {
    Navigate(String),
    Select(String, String),
    Recipe(String, String),
    Expand(String, String),
    More(String, String),
    Operate(String),
    CheckOperation(String),
    Refresh(Uuid),
    Inspect(Uuid, Option<String>),
    Raw(String, String),
    Action(Uuid, Control, BTreeMap<String, SelectionCapture>),
    Retry(Uuid, Invocation, bool),
    Discard(Uuid, u64),
    Candidate(Uuid, u64),
}
impl eframe::App for ExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.events(ctx);
        if let Some(state) = &mut self.state {
            for surface in state.surfaces.values_mut() {
                if let Some(patch) = &mut surface.patch {
                    patch.preparation_paused = self.busy;
                    patch.bind(&self.client);
                    patch.poll(ctx);
                }
            }
        }
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused != self.was_focused {
            self.was_focused = focused;
            if let Some(state) = self.state.as_mut() {
                for surface in state.surfaces.values_mut() {
                    if focused {
                        surface.editor.resume();
                        if let Some(p) = &mut surface.patch {
                            for e in p.editors.values_mut() {
                                e.resume();
                            }
                            if self.busy {
                                p.refresh(ctx);
                            }
                        }
                        surface.loading = false;
                    } else {
                        surface.editor.suspend();
                        if let Some(p) = &mut surface.patch {
                            for e in p.editors.values_mut() {
                                e.suspend();
                            }
                        }
                    }
                }
                if focused && !self.busy {
                    self.load(false, ctx.clone());
                }
            }
        }
        if self.refresh_source
            && !self.busy
            && self.state.as_ref().is_some_and(|s| {
                s.surfaces
                    .values()
                    .all(|s| !s.loading && s.patch.as_ref().is_none_or(|p| !p.loading))
            })
        {
            self.refresh_source = false;
            self.load(false, ctx.clone());
        }
        let mut commands = vec![];
        let composing = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Ime(ImeEvent::Preedit(s)) => Some(!s.is_empty()),
                    egui::Event::Ime(ImeEvent::Commit(_) | ImeEvent::Disabled) => Some(false),
                    _ => None,
                })
                .next_back()
        });
        if let Some(composing) = composing {
            let focus = ctx.memory(|m| m.focused());
            if let Some(state) = self.state.as_mut() {
                for s in state.surfaces.values_mut() {
                    if let Some(p) = &mut s.patch {
                        for e in p.editors.values_mut() {
                            if !composing
                                || e.fields
                                    .keys()
                                    .any(|slot| Some(Id::new((e.id, slot))) == focus)
                            {
                                e.composing = composing;
                            }
                        }
                    }
                    if !composing
                        || s.editor
                            .fields
                            .keys()
                            .any(|slot| Some(Id::new((s.editor.id, slot))) == focus)
                    {
                        s.editor.composing = composing;
                    }
                }
            }
        }
        if ctx.input(|i| i.viewport().close_requested())
            && self
                .state
                .as_ref()
                .is_some_and(|s| s.owners().any(|e| e.dirty() || !e.can_leave()))
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.quit = true;
        }
        TopBottomPanel::top("explorer-toolbar")
            .frame(
                Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(Margin::symmetric(20, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("S")
                            .size(18.)
                            .strong()
                            .color(crate::chrome::ACCENT),
                    );
                    ui.label(RichText::new("Shrine").size(18.).strong());
                    ui.label(
                        RichText::new("/  Explorer")
                            .size(14.)
                            .color(crate::chrome::MUTED),
                    );
                    ui.add_space(20.);
                    if ui
                        .add_enabled(!self.busy, Button::new("Refresh workspace"))
                        .clicked()
                    {
                        self.load(false, ctx.clone());
                    }
                    crate::inspector::pipeline_toggle(ui);
                    if ui.button("Received workspace Goo").clicked()
                        && let Some(s) = &self.state
                    {
                        self.raw = Some((
                            "Received workspace Goo".into(),
                            s.workspace
                                .packet
                                .received_json
                                .as_ref()
                                .unwrap()
                                .to_string(),
                        ));
                    }
                    if self.busy {
                        ui.spinner();
                    }
                    if let Some(op) = &self.operation {
                        ui.label(format!("Mount operation: {op}"));
                        if ui.button("Check mount status").clicked() {
                            commands.push(Command::CheckOperation(op.clone()));
                        }
                    }
                });
            });
        TopBottomPanel::bottom("explorer-status")
            .frame(
                Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if self.busy { "Refreshing" } else { "Workspace" })
                            .small()
                            .color(crate::chrome::ACCENT),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(&self.notice)
                            .small()
                            .color(crate::chrome::MUTED),
                    );
                });
            });
        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(crate::chrome::BACKGROUND)
                    .inner_margin(20),
            )
            .show(ctx, |ui| {
                if let Some(state) = self.state.as_mut() {
                    let nodes = state.workspace.nodes.clone();
                    Self::nodes_ui(ui, &nodes, "", state, &mut commands, &mut self.address);
                } else {
                    ui.label("Waiting for a validated workspace and prepared tree rows…");
                }
            });
        for command in commands {
            match command {
                Command::Navigate(path) => {
                    self.address = path;
                    self.load(true, ctx.clone());
                }
                Command::Select(tree, key) => self.select(tree, key, ctx.clone()),
                Command::Recipe(id, view) => self.recipe(id, view, ctx.clone()),
                Command::Expand(id, key) => {
                    let state = self.state.as_mut().unwrap();
                    let closing = state.trees[&id].expanded.contains(&key);
                    let prefix = format!("{id}|row:{}/", key.trim_end_matches('/'));
                    if closing
                        && state
                            .surfaces
                            .iter()
                            .any(|(id, s)| id.starts_with(&prefix) && s.editor.composing)
                    {
                        self.notice = "Finish composition before collapsing this branch".into();
                        continue;
                    }
                    let loaded = state.trees[&id].next.contains_key(&key);
                    if closing || loaded {
                        let tree = state.trees.get_mut(&id).unwrap();
                        if closing {
                            tree.expanded.remove(&key);
                        } else {
                            tree.expanded.insert(key);
                        }
                        for (id, s) in &mut state.surfaces {
                            if id.starts_with(&prefix) {
                                if closing {
                                    s.editor.suspend();
                                } else {
                                    s.editor.resume();
                                    s.loading = false;
                                }
                            }
                        }
                    } else {
                        self.page(id, key, false, ctx.clone());
                    }
                }
                Command::More(id, key) => self.page(id, key, true, ctx.clone()),
                Command::Operate(key) => self.operate(key, ctx.clone(), false),
                Command::CheckOperation(key) => self.operate(key, ctx.clone(), true),
                Command::Refresh(id) => {
                    if !self.busy {
                        self.refresh(id, ctx.clone());
                    }
                }
                Command::Inspect(id, node) => {
                    if let Some(s) = self.surface_mut(id) {
                        match Inspection::capture(&s.editor, node) {
                            Ok(i) => self.inspection = Some(i),
                            Err(e) => self.notice = e.to_string(),
                        }
                    }
                }
                Command::Raw(title, text) => self.raw = Some((title, text)),
                Command::Retry(id, i, check) => self.send(id, i, check, ctx.clone()),
                Command::Action(id, c, captures) => {
                    if c.r#become && !c.invoke {
                        let valid = self.surface_mut(id).is_some_and(|s| {
                            s.editor.can_act()
                                && s.editor
                                    .presentation
                                    .as_ref()
                                    .is_some_and(|p| p.controls.contains(&c) && c.binding.presented)
                        });
                        if valid {
                            self.change_recipe(id, c.slot.clone(), ctx.clone());
                        }
                        continue;
                    }
                    if let Some(s) = self.surface_mut(id) {
                        match s.editor.activate(&c, &captures) {
                            Ok(i) => self.send(id, i, false, ctx.clone()),
                            Err(e) => self.notice = e.to_string(),
                        }
                    }
                }
                Command::Discard(id, revision) => {
                    if let Some(s) = self.surface_mut(id)
                        && s.editor.can_leave()
                        && s.editor.revision == revision
                    {
                        for f in s.editor.fields.values_mut() {
                            f.value = f.seed.clone();
                        }
                        s.editor.revision += 1;
                    }
                }
                Command::Candidate(id, revision) => {
                    if let Some(s) = self.surface_mut(id)
                        && let Err(e) = s.editor.accept_candidate(revision)
                    {
                        self.notice = e.to_string();
                    }
                }
            }
        }
        if self.pending.is_some() {
            let mut decision = None;
            Window::new("Keep this work?").collapsible(false).show(ctx,|ui|{ui.label("The prepared change would retire an editor with unsaved or unresolved work.");ui.label("Pending saves and composition must be resolved before leaving.");ui.horizontal(|ui|{if ui.button("Stay").clicked(){decision=Some(false);}
if ui.button("Discard draft and continue").clicked(){decision=Some(true);}});});
            if let Some(discard) = decision {
                let pending = self.pending.take().unwrap();
                if discard {
                    if let Err(e) = self.install(pending.state, &pending.guard, true) {
                        self.notice = e.to_string();
                    }
                } else {
                    self.intent += 1;
                    self.notice = "Change refused; current workspace retained".into();
                }
            }
        }
        if let Some(i) = &mut self.inspection {
            let mut open = true;
            i.show(ctx, &mut open);
            if !open {
                self.inspection = None;
            }
        }
        if let Some((title, text)) = &self.raw {
            let mut open = true;
            Window::new(title)
                .open(&mut open)
                .default_size([760., 550.])
                .show(ctx, |ui| {
                    if ui.button("Copy received Goo").clicked() {
                        ctx.copy_text(text.clone());
                    }
                    ScrollArea::both().show(ui, |ui| {
                        ui.monospace(text);
                    });
                });
            if !open {
                self.raw = None;
            }
        }
        if self.quit {
            Window::new("Work retained").show(ctx,|ui|{ui.label("Resolve pending saves and save or discard drafts before closing. Recovery is in memory.");if ui.button("Keep working").clicked(){self.quit=false;}});
        }
        ctx.request_repaint_after(Duration::from_millis(150));
    }
}
pub fn run() -> eframe::Result {
    eframe::run_native(
        "Shrine Explorer",
        eframe::NativeOptions {
            viewport: ViewportBuilder::default().with_inner_size([1280., 850.]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(ExplorerApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    fn state() -> State {
        let mut packet =
            Presentation::decode(include_bytes!("../tests/fixtures/explorer-workspace.json"))
                .unwrap();
        let entry = Entry {
            version: 1,
            target: packet.snapshot.subject.clone(),
            source: "/0x11/app/ios_demo".into(),
            view: packet.view.clone(),
            host: serde_json::from_value(packet.host.clone().unwrap()).unwrap(),
        };
        // Captured packets contain only disposable fixture identities.
        packet.context = crate::model::Context::new(720.);
        let workspace = Workspace::validate(entry, packet).unwrap();
        let p = Presentation::decode(include_bytes!("../tests/fixtures/live-view.json")).unwrap();
        let surface =
            Surface::mounted(&p.snapshot.subject.clone(), "slot", "/sys/slot/content", p).unwrap();
        State {
            specs: specs(&workspace.nodes),
            workspace,
            trees: BTreeMap::new(),
            selections: BTreeMap::new(),
            surfaces: BTreeMap::from([("editor".into(), surface)]),
            choices: BTreeMap::new(),
            operations: BTreeMap::new(),
        }
    }
    #[test]
    fn compatible_refresh_retains_draft_and_owner() {
        let mut old = state();
        let field = old.surfaces["editor"]
            .editor
            .fields
            .keys()
            .next()
            .unwrap()
            .clone();
        old.surfaces
            .get_mut("editor")
            .unwrap()
            .editor
            .edit(&field, "working draft".into());
        let id = old.surfaces["editor"].editor.id;
        let mut next = state();
        assert_eq!(old.affected(&next).count(), 0);
        next.merge(&old).unwrap();
        assert_eq!(next.surfaces["editor"].editor.id, id);
        assert_eq!(
            next.surfaces["editor"].editor.fields[&field].value,
            "working draft"
        );
    }
    #[test]
    fn replacement_is_guarded_and_does_not_inherit_owner() {
        let old = state();
        let mut next = state();
        next.surfaces
            .get_mut("editor")
            .unwrap()
            .editor
            .presentation
            .as_mut()
            .unwrap()
            .snapshot
            .life = "replacement".into();
        assert_eq!(old.affected(&next).count(), 1);
        next.merge(&old).unwrap();
        assert_ne!(
            old.surfaces["editor"].editor.id,
            next.surfaces["editor"].editor.id
        );
    }
    #[test]
    fn pending_save_survives_compatible_refresh_and_completion_targets_owner() {
        let mut old = state();
        let e = &mut old.surfaces.get_mut("editor").unwrap().editor;
        let control = e
            .presentation
            .as_ref()
            .unwrap()
            .controls
            .iter()
            .find(|c| c.binding.presented && c.binding.kind == "invoke")
            .unwrap()
            .clone();
        let i = e.activate(&control, &BTreeMap::new()).unwrap();
        let mut next = state();
        next.merge(&old).unwrap();
        let e = &mut next.surfaces.get_mut("editor").unwrap().editor;
        assert_eq!(e.pending.as_ref().unwrap().body(), i.body());
        assert!(!e.complete("wrong-id", Outcome::Accepted));
        assert!(e.complete(i.id(), Outcome::Accepted));
        assert!(e.stale);
    }
    #[test]
    fn nested_panes_resolve_shared_selection_and_reordering_retains_identity() {
        let state = state();
        let selection = state
            .specs
            .iter()
            .find(|s| node_text(&s.node, "kind") == "selection")
            .unwrap();
        for consumer in state
            .specs
            .iter()
            .filter(|s| ["follow", "topos"].contains(&node_text(&s.node, "kind")))
        {
            assert_eq!(consumer.selection.as_ref(), Some(&selection.id));
        }
        let mut nodes = state.workspace.nodes.clone();
        fn reverse(nodes: &mut [Value]) {
            for n in nodes {
                if n["kind"] == "split" {
                    n["children"].as_array_mut().unwrap().reverse();
                }
                if let Some(c) = n["children"].as_array_mut() {
                    reverse(c);
                }
            }
        }
        reverse(&mut nodes);
        let a: BTreeSet<_> = state
            .specs
            .iter()
            .filter(|s| !node_text(&s.node, "key").is_empty())
            .map(|s| s.id.clone())
            .collect();
        let b: BTreeSet<_> = specs(&nodes)
            .into_iter()
            .filter(|s| !node_text(&s.node, "key").is_empty())
            .map(|s| s.id)
            .collect();
        assert_eq!(a, b);
    }
    #[test]
    fn late_structural_response_cannot_replace_newer_intent_or_work() {
        let (tx, rx) = mpsc::channel();
        let old = state();
        let basis = old.owners().map(|e| (e.id, e.revision)).collect();
        let mut app = ExplorerApp {
            client: Client::new("http://127.0.0.1:1").unwrap(),
            endpoint: "/goo-inspector".into(),
            address: "/old".into(),
            state: Some(old),
            intent: 2,
            pending: None,
            stage_basis: Some(basis),
            refresh_source: false,
            was_focused: true,
            busy: true,
            tx,
            rx,
            notice: String::new(),
            inspection: None,
            raw: None,
            operation: None,
            quit: false,
        };
        let mut candidate = state();
        candidate.workspace.entry.source = "/replacement".into();
        app.tx
            .send(Event::Stage(1, Ok(Box::new(candidate.clone()))))
            .unwrap();
        app.events(&egui::Context::default());
        assert_eq!(app.address, "/old");
        app.state
            .as_mut()
            .unwrap()
            .surfaces
            .get_mut("editor")
            .unwrap()
            .editor
            .revision += 1;
        app.tx
            .send(Event::Stage(2, Ok(Box::new(candidate))))
            .unwrap();
        app.events(&egui::Context::default());
        assert_eq!(app.address, "/old");
        assert!(app.notice.contains("superseded"));
    }
}
