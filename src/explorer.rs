//! Portable Explorer contracts and transactional tree admission. No HTML is consumed.
use crate::{model::*, session::Editor, transport::Client};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Navigation {
    pub kind: String,
    pub source_slot: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Operation {
    pub key: String,
    pub kind: String,
    pub source: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Host {
    pub version: u32,
    pub kind: String,
    pub declaration: String,
    pub navigation: Navigation,
    pub operations: Vec<Operation>,
}
impl Host {
    pub fn validate(&self, view: &str) -> Result<()> {
        ensure!(
            self.version == 1 && self.kind == "workspace" && self.declaration == view && path(view),
            "Invalid workspace host"
        );
        ensure!(
            self.navigation.kind == "workspace-session" && path(&self.navigation.source_slot),
            "Unsupported navigation"
        );
        let mut keys = BTreeSet::new();
        for op in &self.operations {
            ensure!(
                path(&op.key)
                    && keys.insert(&op.key)
                    && op.kind == "mount-rescan"
                    && path(&op.source),
                "Unsupported/duplicate host operation"
            );
        }
        Ok(())
    }
}
pub fn path(s: &str) -> bool {
    s == "/"
        || (s.starts_with('/')
            && !s.ends_with('/')
            && !s.contains("//")
            && !s.chars().any(char::is_whitespace)
            && !s.split('/').any(|v| v == "." || v == ".."))
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Entry {
    pub version: u32,
    pub target: String,
    pub source: String,
    pub view: String,
    pub host: Host,
}
impl Entry {
    pub fn validate(&self, source: &str) -> Result<()> {
        ensure!(
            self.version == 1 && self.source == source && path(source),
            "Workspace entry context mismatch"
        );
        let parts: Vec<_> = self.target.split('/').collect();
        ensure!(
            parts.len() == 5
                && parts[2] == "goo"
                && parts[3] == "ses"
                && parts[4].parse::<u64>().is_ok(),
            "Workspace did not allocate a session"
        );
        self.host.validate(&self.view)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Layout {
    pub size: String,
    pub render: Presentation,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub version: u32,
    pub ok: bool,
    pub layout_policy: String,
    pub context: Context,
    pub layouts: Vec<Layout>,
}
impl Bundle {
    pub fn validate(&mut self, target: &str) -> Result<()> {
        ensure!(
            self.version == 1
                && self.ok
                && self.layout_policy == "pane-v1"
                && self.context == Context::new(240.),
            "Invalid layout bundle"
        );
        ensure!(self.layouts.len() == 3, "Incomplete layout bundle");
        let mut basis = None;
        for (l, (name, width)) in
            self.layouts
                .iter_mut()
                .zip([("sm", 240.), ("md", 480.), ("lg", 720.)])
        {
            ensure!(l.size == name, "Invalid layout order");
            l.render.validate(&Context::new(width), target)?;
            let identity = l.render.lifetime();
            ensure!(
                basis.as_ref().is_none_or(|b| b == &identity),
                "Mixed layout lifetimes"
            );
            basis = Some(identity);
            if l.render.received_json.is_none() {
                l.render.received_json = Some(serde_json::to_string(&l.render)?.into());
            }
        }
        Ok(())
    }
    pub fn at(&self, width: f64) -> &Presentation {
        &self
            .layouts
            .iter()
            .rev()
            .find(|l| l.render.context.width <= width)
            .unwrap_or(&self.layouts[0])
            .render
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeMember {
    #[serde(flatten)]
    pub member: Member,
    pub parent: Option<String>,
    pub has_children: bool,
    pub topology: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PreparedMember {
    pub key: String,
    pub slot: String,
    pub bundle: Bundle,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Page {
    #[serde(skip)]
    pub received_json: Option<std::sync::Arc<str>>,
    pub id: String,
    pub life: String,
    pub axis: String,
    pub topology: String,
    pub r#type: String,
    pub incremental: bool,
    pub bounded: bool,
    pub epoch: String,
    pub parent: String,
    pub next: Option<String>,
    pub omitted: usize,
    pub members: Vec<TreeMember>,
    pub presentations: Vec<PreparedMember>,
}
impl Page {
    pub fn validate(
        &mut self,
        source: &str,
        axis: &str,
        slot: &str,
        parent: Option<&str>,
        epoch: Option<&str>,
    ) -> Result<()> {
        ensure!(
            self.id == source
                && self.axis == axis
                && self.topology == "tree"
                && self.r#type == "namespace-record"
                && self.incremental
                && self.bounded
                && !self.life.is_empty()
                && !self.epoch.is_empty(),
            "Invalid tree page identity"
        );
        ensure!(
            self.parent == parent.unwrap_or(source) && epoch.is_none_or(|e| e == self.epoch),
            "Stale tree epoch/parent"
        );
        ensure!(
            self.members.len() <= if parent.is_none() { 65 } else { 64 } && self.omitted <= 64,
            "Oversized tree page"
        );
        let mut keys = BTreeSet::new();
        for n in &self.members {
            let m = &n.member;
            let root = parent.is_none() && m.key == source && n.parent.is_none();
            ensure!(
                keys.insert(m.key.clone())
                    && path(&m.key)
                    && m.key == m.subject
                    && m.source == source
                    && m.source_life == self.life
                    && m.axis == axis
                    && m.r#type == "namespace-record"
                    && !m.life.is_empty()
                    && !m.expected.is_empty()
                    && n.topology == "tree"
                    && ["available", "unavailable"].contains(&m.status.as_str()),
                "Invalid tree member"
            );
            let prefix = format!("{}/", self.parent.trim_end_matches('/'));
            ensure!(
                root || (n.parent.as_deref() == Some(&self.parent)
                    && m.key
                        .strip_prefix(&prefix)
                        .is_some_and(|s| !s.is_empty() && !s.contains('/'))),
                "Tree member escaped parent"
            );
        }
        if parent.is_none() {
            ensure!(
                self.members
                    .iter()
                    .any(|m| m.member.key == source && m.parent.is_none()),
                "Missing tree root"
            );
        }
        let mut prepared = BTreeSet::new();
        for p in &mut self.presentations {
            let m = self
                .members
                .iter()
                .find(|m| m.member.key == p.key)
                .ok_or_else(|| anyhow::anyhow!("Orphan row presentation"))?;
            ensure!(
                p.slot == slot && prepared.insert(p.key.clone()) && m.member.status == "available",
                "Duplicate or unavailable prepared row"
            );
            p.bundle.validate(&m.member.subject)?;
            for l in &p.bundle.layouts {
                ensure!(
                    l.render.snapshot.life == m.member.life
                        && l.render.snapshot.expected == m.member.expected,
                    "Row preparation/member evidence mismatch"
                );
            }
        }
        ensure!(
            self.members
                .iter()
                .filter(|m| m.member.status == "available")
                .all(|m| prepared.contains(&m.member.key)),
            "Missing prepared row"
        );
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Tree {
    pub source: String,
    pub axis: String,
    pub slot: String,
    pub life: String,
    pub epoch: String,
    pub members: BTreeMap<String, TreeMember>,
    pub order: Vec<String>,
    pub bundles: BTreeMap<String, Bundle>,
    pub next: BTreeMap<String, Option<String>>,
    pub expanded: BTreeSet<String>,
    pub omitted: BTreeMap<String, usize>,
}
impl Tree {
    pub fn new(source: String, axis: String, slot: String) -> Self {
        Self {
            source,
            axis,
            slot,
            life: String::new(),
            epoch: String::new(),
            members: BTreeMap::new(),
            order: vec![],
            bundles: BTreeMap::new(),
            next: BTreeMap::new(),
            expanded: BTreeSet::new(),
            omitted: BTreeMap::new(),
        }
    }
    /// Validates and stages the complete candidate. The installed tree is unchanged on error.
    pub fn with_page(
        &self,
        mut page: Page,
        parent: Option<&str>,
        after: Option<&str>,
    ) -> Result<Self> {
        page.validate(
            &self.source,
            &self.axis,
            &self.slot,
            parent,
            if parent.is_some() {
                Some(&self.epoch)
            } else {
                None
            },
        )?;
        if let Some(parent) = parent {
            ensure!(
                page.life == self.life && self.members.contains_key(parent),
                "Tree source/parent replaced"
            );
            if let Some(after) = after {
                ensure!(
                    self.next.get(parent).and_then(|v| v.as_deref()) == Some(after)
                        && page.next.as_deref() != Some(after),
                    "Stale/repeated continuation"
                );
            }
        }
        let mut next = if parent.is_none() {
            Self::new(self.source.clone(), self.axis.clone(), self.slot.clone())
        } else {
            self.clone()
        };
        next.life = page.life;
        next.epoch = page.epoch;
        for n in page.members {
            let key = n.member.key.clone();
            ensure!(
                !next.members.contains_key(&key),
                "Duplicate materialized member"
            );
            next.order.push(key.clone());
            next.members.insert(key, n);
        }
        ensure!(
            next.members.len() <= 4096,
            "Tree materialization limit (4096); open a smaller subtree"
        );
        for p in page.presentations {
            next.bundles.insert(p.key, p.bundle);
        }
        next.expanded.insert(page.parent.clone());
        next.omitted.insert(page.parent.clone(), page.omitted);
        next.next.insert(page.parent, page.next);
        Ok(next)
    }
}
#[derive(Clone, Debug)]
pub struct Workspace {
    pub entry: Entry,
    pub packet: Presentation,
    pub nodes: Vec<Value>,
}
fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
pub fn node_text<'a>(v: &'a Value, k: &str) -> &'a str {
    text(v, k)
}
pub fn children(v: &Value) -> &[Value] {
    v["children"].as_array().map(Vec::as_slice).unwrap_or(&[])
}
impl Workspace {
    pub fn validate(entry: Entry, packet: Presentation) -> Result<Self> {
        // Host authority is admitted separately; ordinary surfaces never inherit it.
        entry.host.validate(&entry.view)?;
        let host: Host = serde_json::from_value(
            packet
                .host
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Missing installed host"))?,
        )?;
        ensure!(
            host == entry.host && packet.view == entry.view,
            "Workspace host changed during entry"
        );
        let mut envelope = packet.clone();
        envelope.host = None;
        envelope.validate_envelope(&packet.context, &entry.target)?;
        ensure!(
            packet.kind == "pane" && packet.controls.is_empty() && packet.display.is_none(),
            "Expected workspace pane"
        );
        let raw: Value = serde_json::from_str(
            packet
                .received_json
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing original workspace packet"))?,
        )?;
        let nodes = raw["description"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing structure"))?
            .clone();
        let mut count = 0;
        fn walk(
            nodes: &[Value],
            scope: &BTreeMap<String, (String, String)>,
            host: &Host,
            packet: &Presentation,
            count: &mut usize,
        ) -> Result<()> {
            let mut siblings = BTreeSet::new();
            for n in nodes {
                *count += 1;
                ensure!(*count <= 512, "Workspace exceeds node limit");
                let kind = text(n, "kind");
                let key = text(n, "key");
                if !key.is_empty() {
                    ensure!(siblings.insert(key), "Duplicate sibling identity");
                }
                let mut scope = scope.clone();
                match kind {
                    "pane" | "split" | "pane_stack" => {}
                    "or" => {
                        let alternatives = children(n);
                        ensure!(
                            alternatives.len() == 2
                                && text(&alternatives[0], "kind") == "follow"
                                && text(&alternatives[1], "kind") == "native",
                            "Unsupported pane fallback; expected follower and prepared fallback"
                        );
                    }
                    "selection" => {
                        ensure!(
                            !key.is_empty()
                                && path(text(n, "source"))
                                && text(n, "topology") == "tree"
                                && ["x", "y", "z", ""].contains(&text(n, "axis")),
                            "Unsupported selection"
                        );
                        scope.insert(
                            key.into(),
                            (
                                text(n, "source").into(),
                                if text(n, "axis").is_empty() {
                                    "z".into()
                                } else {
                                    text(n, "axis").into()
                                },
                            ),
                        );
                    }
                    "topos" => {
                        ensure!(
                            ["list_tree", "diagram_tree"].contains(&text(n, "renderer"))
                                && scope.contains_key(text(n, "selection"))
                                && path(text(n, "slot")),
                            "Unsupported/unscoped tree"
                        );
                        let (source, axis) = &scope[text(n, "selection")];
                        ensure!(
                            text(n, "source") == source
                                && (text(n, "axis").is_empty() || text(n, "axis") == axis)
                                && ["", "tree"].contains(&text(n, "topology")),
                            "Tree/selection source mismatch"
                        );
                    }
                    "follow" => {
                        ensure!(
                            scope.contains_key(text(n, "selection")) && path(text(n, "slot")),
                            "Unscoped follower"
                        );
                    }
                    "navigate" => {
                        ensure!(
                            ["input", "parent", ""].contains(&text(n, "mode"))
                                && path(text(n, "source")),
                            "Unsupported navigation"
                        );
                    }
                    "operation" | "operation_status" => {
                        ensure!(
                            host.operations
                                .iter()
                                .any(|o| o.key == text(n, "operation")),
                            "Undeclared host operation"
                        );
                    }
                    "native" => {
                        let route = text(n, "node");
                        ensure!(
                            !route.is_empty() && route.split('.').all(|s| s.parse::<u32>().is_ok()),
                            "Invalid native declaration route"
                        );
                        let surface = native(packet, n)?;
                        ensure!(
                            surface.view == packet.view
                                && surface.controls.iter().all(|c| !c.invoke && !c.r#become),
                            "Workspace native surface cannot grant domain authority"
                        );
                        surface.validate(&packet.context, &packet.snapshot.subject)?;
                    }
                    _ => bail!("Unsupported workspace component: {kind}"),
                }
                walk(children(n), &scope, host, packet, count)?;
            }
            Ok(())
        }
        walk(&nodes, &BTreeMap::new(), &entry.host, &packet, &mut count)?;
        Ok(Self {
            entry,
            packet,
            nodes,
        })
    }
}
pub fn native(packet: &Presentation, n: &Value) -> Result<Presentation> {
    let mut raw = serde_json::to_value(packet)?;
    for (k, v) in n
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Invalid native node"))?
    {
        raw[k] = v.clone();
    }
    raw["kind"] = json!("view");
    raw["host"] = Value::Null;
    raw["description"] = Value::Null;
    Presentation::decode(&serde_json::to_vec(&raw)?)
}
/// A consent snapshot is tied to intent, owner incarnation and work revision.
#[derive(Clone, Debug)]
pub struct Guard {
    pub intent: u64,
    pub stamps: Vec<(uuid::Uuid, u64)>,
}
impl Guard {
    pub fn capture<'a>(intent: u64, owners: impl Iterator<Item = &'a Editor>) -> Self {
        Self {
            intent,
            stamps: owners.map(|e| (e.id, e.revision)).collect(),
        }
    }
    pub fn check<'a>(
        &self,
        intent: u64,
        owners: impl Iterator<Item = &'a Editor>,
        discard: bool,
    ) -> Result<()> {
        let owners: Vec<_> = owners.collect();
        ensure!(
            self.intent == intent
                && self.stamps
                    == owners
                        .iter()
                        .map(|e| (e.id, e.revision))
                        .collect::<Vec<_>>(),
            "Transition superseded by new work"
        );
        ensure!(
            owners
                .iter()
                .all(|e| e.can_leave() && (discard || !e.dirty())),
            "Resolve pending saves/composition or keep/discard drafts"
        );
        Ok(())
    }
}
impl Client {
    pub fn workspace_entry(&self, endpoint: &str, source: &str) -> Result<Entry> {
        ensure!(
            endpoint.starts_with('/')
                && !endpoint.starts_with("//")
                && !endpoint.contains(['?', '#']),
            "Invalid workspace endpoint"
        );
        let v = self.explorer_json(
            endpoint,
            false,
            vec![
                ("format".into(), "json".into()),
                ("path".into(), source.into()),
            ],
        )?;
        let entry: Entry = serde_json::from_value(v)?;
        entry.validate(source)?;
        Ok(entry)
    }
    pub fn workspace(&self, entry: Entry) -> Result<Workspace> {
        let p = self.prepared(
            vec![
                ("target".into(), entry.target.clone()),
                ("view".into(), entry.view.clone()),
                ("workspace".into(), entry.view.clone()),
            ],
            720.,
        )?;
        Workspace::validate(entry, p)
    }
    pub fn prepared(&self, mut fields: Vec<(String, String)>, width: f64) -> Result<Presentation> {
        fields.extend([
            ("width".into(), width.to_string()),
            ("textScale".into(), "1".into()),
            ("direction".into(), "ltr".into()),
        ]);
        let bytes = self.explorer_bytes("/goo/render", true, fields)?;
        Presentation::decode(&bytes)
    }
    pub fn tree_page(
        &self,
        tree: &Tree,
        parent: Option<&str>,
        after: Option<&str>,
    ) -> Result<Page> {
        let mut fields = vec![
            ("target".into(), tree.source.clone()),
            ("axis".into(), tree.axis.clone()),
            ("topology".into(), "tree".into()),
            ("paged".into(), "1".into()),
            ("presentations".into(), "pane-v1".into()),
            ("slot".into(), tree.slot.clone()),
            ("textScale".into(), "1".into()),
            ("direction".into(), "ltr".into()),
        ];
        if let Some(parent) = parent {
            fields.extend([
                ("parent".into(), parent.into()),
                ("epoch".into(), tree.epoch.clone()),
            ]);
        }
        if let Some(after) = after {
            fields.push(("after".into(), after.into()));
        }
        let bytes = self.explorer_bytes("/goo/source", false, fields)?;
        let raw: Value = serde_json::from_slice(&bytes)?;
        let mut page: Page = serde_json::from_slice(&bytes)?;
        page.received_json = Some(std::str::from_utf8(&bytes)?.into());
        for (packet, original) in page
            .presentations
            .iter_mut()
            .zip(raw["presentations"].as_array().unwrap())
        {
            for (layout, original) in packet
                .bundle
                .layouts
                .iter_mut()
                .zip(original["bundle"]["layouts"].as_array().unwrap())
            {
                layout.render = Presentation::decode(&serde_json::to_vec(&original["render"])?)?;
            }
        }
        Ok(page)
    }
}
