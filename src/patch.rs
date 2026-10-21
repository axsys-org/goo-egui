//! Portable patch admission and immutable offered operations. Never consumes HTML.
use crate::{
    model::*,
    session::Editor,
    transport::{Client, Invocation, Outcome, SelectionCapture},
};
use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
pub fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
pub fn array<'a>(v: &'a Value, k: &str) -> Result<&'a Vec<Value>> {
    v[k].as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing patch {k}"))
}
pub fn number(v: &Value, k: &str) -> Result<f64> {
    let x = v[k]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("Missing number {k}"))?;
    ensure!(
        x.is_finite() && x.abs() < 1e6,
        "Invalid patch coordinate {k}"
    );
    Ok(x)
}
fn identity(v: &Value) -> Result<()> {
    for k in ["key", "lifetime", "selectorRef"] {
        ensure!(!text(v, k).is_empty(), "Missing occurrence {k}");
    }
    Ok(())
}
pub fn rect(v: &Value) -> Result<Box2> {
    let b: Box2 = serde_json::from_value(v.clone())?;
    ensure!(b.valid(), "Invalid patch rectangle");
    Ok(b)
}
pub fn node_target(v: &Value) -> Value {
    json!({"kind":"node","node":v["key"],"lifetime":v["lifetime"]})
}
pub fn port_target(n: &Value, p: &Value) -> Value {
    json!({"kind":"port","node":n["key"],"nodeLifetime":n["lifetime"],"port":p["key"],"lifetime":p["lifetime"]})
}
pub fn connection_target(c: &Value) -> Value {
    json!({"kind":"connection","connection":c["key"],"lifetime":c["lifetime"]})
}
#[derive(Clone)]
pub struct Packet {
    pub root: Presentation,
    pub descriptor: Value,
    pub scene: Value,
    pub sequence: u64,
    pub covered: BTreeSet<String>,
    pub bodies: BTreeMap<String, Presentation>,
    pub references: BTreeMap<String, Value>,
    pub offers: BTreeMap<String, (String, Control, String)>,
}
impl Packet {
    pub fn admit(root: Presentation) -> Result<Self> {
        root.validate_envelope(&root.context, &root.snapshot.subject)?;
        ensure!(
            root.kind == "pane"
                && root.display.is_none()
                && root.controls.is_empty()
                && root.host.is_none(),
            "Expected portable patch pane"
        );
        let raw: Value = serde_json::from_str(
            root.received_json
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing received patch"))?,
        )?;
        fn find(v: &Value, out: &mut Vec<Value>) -> Result<()> {
            for n in v
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Missing patch description"))?
            {
                match text(n, "kind") {
                    "patch" => out.push(n.clone()),
                    "pane" | "split" | "pane_stack" => find(&n["children"], out)?,
                    _ => bail!("Unsupported patch wrapper"),
                }
            }
            Ok(())
        }
        let mut nodes = vec![];
        find(&raw["description"], &mut nodes)?;
        ensure!(nodes.len() == 1, "Expected one patch scene");
        let descriptor = nodes.remove(0);
        ensure!(
            descriptor["contentVersion"] == 1,
            "Unsupported patch content version"
        );
        let scene = descriptor["scene"].clone();
        let h = &scene["header"];
        ensure!(h["protocol"] == 1, "Unsupported patch protocol");
        for k in [
            "surface",
            "encounter",
            "sourceRev",
            "layoutRev",
            "rendererEpoch",
        ] {
            ensure!(!text(h, k).is_empty(), "Missing scene identity {k}");
        }
        ensure!(h["viewSeq"].as_u64().is_some(), "Invalid scene ordering");
        rect(&scene["extent"])?;
        let sequence = raw["snapshot"]["projectionSeq"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing complete projection sequence"))?;
        let covered = array(&raw["snapshot"], "coveredOperationIds")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| anyhow::anyhow!("Invalid coverage"))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        let ns = array(&scene, "nodes")?;
        let cs = array(&scene, "connections")?;
        ensure!(
            ns.len() <= 4096 && cs.len() <= 8192,
            "Patch bounds exceeded"
        );
        let mut targets = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for n in ns {
            identity(n)?;
            ensure!(keys.insert(text(n, "key")), "Duplicate node");
            rect(&n["rect"])?;
            number(n, "z")?;
            ensure!(
                ["node", "proxy"].contains(&text(n, "presentation")),
                "Unknown node presentation"
            );
            ensure!(n["draggable"].is_boolean(), "Missing movement policy");
            targets.insert(portable_key(&node_target(n)));
            let mut ports = BTreeSet::new();
            for p in array(n, "ports")? {
                identity(p)?;
                ensure!(ports.insert(text(p, "key")), "Duplicate port");
                for k in ["anchor", "outward"] {
                    number(&p[k], "x")?;
                    number(&p[k], "y")?;
                }
                rect(&p["hitRect"])?;
                ensure!(p["connectable"].is_boolean(), "Missing port policy");
                targets.insert(portable_key(&port_target(n, p)));
            }
        }
        let mut connkeys = BTreeSet::new();
        for c in cs {
            identity(c)?;
            ensure!(
                connkeys.insert(text(c, "key")) && !text(c, "topologyRef").is_empty(),
                "Invalid connection identity"
            );
            number(c, "z")?;
            let mut ends = BTreeSet::new();
            for e in array(c, "endpoints")? {
                ensure!(
                    ends.insert(text(e, "key")) && targets.contains(&portable_key(&e["port"])),
                    "Invalid connection endpoint"
                );
            }
            let mut junctions = BTreeSet::new();
            for j in array(c, "junctions")? {
                ensure!(junctions.insert(text(j, "key")), "Duplicate junction");
                number(&j["point"], "x")?;
                number(&j["point"], "y")?;
            }
            let mut parts = BTreeSet::new();
            for p in array(c, "parts")? {
                identity(p)?;
                ensure!(parts.insert(text(p, "key")), "Duplicate connection part");
            }
            let mut labels = BTreeSet::new();
            for l in array(c, "labels")? {
                ensure!(labels.insert(text(l, "key")), "Duplicate connection label");
                rect(&l["rect"])?;
            }
            let mut segments = BTreeSet::new();
            for s in array(c, "segments")? {
                ensure!(segments.insert(text(s, "key")), "Duplicate route");
                for k in ["from", "to"] {
                    let e = &s[k];
                    ensure!(
                        match text(e, "kind") {
                            "endpoint" => ends.contains(text(e, "key")),
                            "junction" => junctions.contains(text(e, "key")),
                            _ => false,
                        },
                        "Dangling route endpoint"
                    );
                }
                let cmds = array(s, "commands")?;
                ensure!(
                    !cmds.is_empty() && cmds.len() <= 4096,
                    "Invalid route length"
                );
                for (i, cmd) in cmds.iter().enumerate() {
                    ensure!(
                        if i == 0 {
                            text(cmd, "op") == "M"
                        } else {
                            ["L", "C"].contains(&text(cmd, "op"))
                        },
                        "Invalid route command"
                    );
                    number(&cmd["to"], "x")?;
                    number(&cmd["to"], "y")?;
                    if text(cmd, "op") == "C" {
                        for k in ["c1", "c2"] {
                            number(&cmd[k], "x")?;
                            number(&cmd[k], "y")?;
                        }
                    }
                }
                for k in ["startMarker", "endMarker"] {
                    ensure!(
                        ["", "none", "dot", "arrow"].contains(&text(s, k)),
                        "Unsupported route marker"
                    );
                }
            }
        }
        let policy = &scene["policy"];
        for k in [
            "minScale",
            "maxScale",
            "dragThresholdPx",
            "portTolerancePx",
            "wireTolerancePx",
            "snapTolerancePx",
            "touchDragThresholdPx",
            "touchPortTolerancePx",
            "autoPanBandPx",
            "autoPanMaxPxPerSecond",
        ] {
            ensure!(number(policy, k)? >= 0., "Invalid interaction policy");
        }
        ensure!(
            number(policy, "minScale")? > 0.
                && number(policy, "maxScale")? >= number(policy, "minScale")?,
            "Invalid zoom policy"
        );
        ensure!(
            policy["alignmentSnap"].is_boolean(),
            "Missing alignment policy"
        );
        if let Some(grid) = policy.get("grid") {
            number(&grid["origin"], "x")?;
            number(&grid["origin"], "y")?;
            ensure!(
                number(&grid["step"], "x")? > 0. && number(&grid["step"], "y")? > 0.,
                "Invalid snap grid"
            );
        }
        let camera = &scene["initialCamera"];
        if camera["kind"] == "fit" {
            ensure!(number(camera, "paddingPx")? >= 0., "Invalid camera padding");
        } else {
            number(camera, "tx")?;
            number(camera, "ty")?;
            let scale = number(camera, "scale")?;
            ensure!(
                scale >= number(policy, "minScale")? && scale <= number(policy, "maxScale")?,
                "Initial camera outside policy"
            );
        }
        let mut groups = BTreeSet::new();
        for group in array(&descriptor, "connectionControls")? {
            let key = text(group, "key");
            ensure!(groups.insert(key), "Duplicate connection controls");
            let c = cs
                .iter()
                .find(|c| text(c, "key") == key)
                .ok_or_else(|| anyhow::anyhow!("Controls reference absent connection"))?;
            for control in array(group, "controls")? {
                ensure!(
                    text(control, "connection") == key && !text(control, "label").is_empty(),
                    "Invalid connection control"
                );
                match text(control, "kind") {
                    "disconnect" => {}
                    "reconnect" => ensure!(
                        array(c, "endpoints")?
                            .iter()
                            .any(|e| e["key"] == control["endpoint"]),
                        "Unknown reconnect endpoint"
                    ),
                    _ => bail!("Unknown connection control"),
                }
            }
        }
        let mut bodies = BTreeMap::new();
        let mut references = BTreeMap::new();
        let mut contentkeys = BTreeSet::new();
        let contents = array(&descriptor, "contents")?;
        for c in contents {
            let kind = text(c, "kind");
            let key = text(c, "key");
            ensure!(contentkeys.insert((kind, key)), "Duplicate patch content");
            ensure!(
                ["patch_node", "patch_connection", "patch_operation"].contains(&kind),
                "Unknown occurrence content"
            );
            if kind == "patch_node" {
                ensure!(keys.contains(key), "Body has no node");
            }
            for item in array(c, "content")? {
                ensure!(
                    match kind {
                        "patch_node" => ["body", "port", "handle", "select", "presentation"]
                            .contains(&text(item, "kind")),
                        "patch_connection" => text(item, "kind") == "label",
                        "patch_operation" => text(item, "kind") == "body",
                        _ => false,
                    },
                    "Unsupported {kind} content arrangement"
                );
                if text(item, "kind") == "label" {
                    ensure!(
                        array(item, "children")?
                            .iter()
                            .all(|v| ["body", "presentation"].contains(&text(v, "kind"))),
                        "Unsupported nested label content"
                    );
                }
            }
            fn body(
                root: &Presentation,
                items: &[Value],
                prefix: &str,
                bodies: &mut BTreeMap<String, Presentation>,
                references: &mut BTreeMap<String, Value>,
            ) -> Result<()> {
                for b in items {
                    match text(b, "kind") {
                        "body" => {
                            let mut p = root.clone();
                            p.kind = "view".into();
                            p.description = None;
                            p.context = serde_json::from_value(b["context"].clone())?;
                            ensure!(
                                p.context.text_scale == root.context.text_scale
                                    && p.context.direction == root.context.direction,
                                "Body text context differs from scene"
                            );
                            p.profile = serde_json::from_value(b["profile"].clone())?;
                            p.display = Some(serde_json::from_value(b["display"].clone())?);
                            p.controls = serde_json::from_value(b["controls"].clone())?;
                            p.validate(&p.context, &p.snapshot.subject)?;
                            ensure!(
                                p.controls.iter().all(|c| !c.r#become),
                                "Recipe transitions inside patch bodies are not supported"
                            );
                            ensure!(
                                bodies
                                    .insert(format!("{prefix}/{}", text(b, "key")), p)
                                    .is_none(),
                                "Duplicate body route"
                            );
                        }
                        "port" | "select" => {
                            ensure!(
                                !text(b, "key").is_empty() && array(b, "moves")?.is_empty(),
                                "Invalid passive affordance"
                            );
                        }
                        "handle" => {
                            ensure!(!text(b, "key").is_empty(), "Invalid handle");
                            for m in array(b, "moves")? {
                                number(m, "dx")?;
                                number(m, "dy")?;
                                ensure!(!text(m, "label").is_empty(), "Unlabelled movement");
                            }
                        }
                        "label" => body(
                            root,
                            array(b, "children")?,
                            &format!("{prefix}/label:{}", text(b, "key")),
                            bodies,
                            references,
                        )?,
                        "presentation" => {
                            ensure!(
                                crate::explorer::path(text(b, "target"))
                                    && crate::explorer::path(text(b, "slot"))
                                    && b["units"] == "css-px",
                                "Invalid embedded presentation reference"
                            );
                            rect(&b["rect"])?;
                            ensure!(
                                references
                                    .insert(format!("{prefix}/ref:{}", text(b, "key")), b.clone())
                                    .is_none(),
                                "Duplicate embedded reference"
                            );
                        }
                        kind => bail!("Unsupported portable content {kind}"),
                    }
                }
                Ok(())
            }
            if kind == "patch_node" {
                let n = ns.iter().find(|n| text(n, "key") == key).unwrap();
                let mut ports = BTreeSet::new();
                for item in array(c, "content")? {
                    if text(item, "kind") == "port" {
                        ensure!(
                            ports.insert(text(item, "key"))
                                && array(n, "ports")?.iter().any(|p| p["key"] == item["key"]),
                            "Dangling/duplicate port content"
                        );
                    }
                }
            }
            body(
                &root,
                array(c, "content")?,
                &format!("{kind}:{key}"),
                &mut bodies,
                &mut references,
            )?;
        }
        for n in ns {
            ensure!(
                contentkeys.contains(&("patch_node", text(n, "key"))),
                "Missing node content"
            );
            let c = contents
                .iter()
                .find(|c| text(c, "kind") == "patch_node" && text(c, "key") == text(n, "key"))
                .unwrap();
            for p in array(n, "ports")? {
                ensure!(
                    array(c, "content")?
                        .iter()
                        .any(|b| text(b, "kind") == "port" && text(b, "key") == text(p, "key")),
                    "Missing portable port"
                );
            }
        }
        for c in cs {
            let content = contents
                .iter()
                .find(|v| text(v, "kind") == "patch_connection" && v["key"] == c["key"])
                .ok_or_else(|| anyhow::anyhow!("Missing connection content"))?;
            for label in array(c, "labels")? {
                ensure!(
                    array(content, "content")?
                        .iter()
                        .any(|v| text(v, "kind") == "label" && v["key"] == label["key"]),
                    "Missing connection label content"
                );
            }
        }
        let mut offers = BTreeMap::new();
        for o in array(&descriptor, "offerBindings")? {
            let kind = text(o, "kind");
            let control: Control = serde_json::from_value(o["control"].clone())?;
            let slot = text(o, "argumentSlot");
            let body = bodies
                .iter()
                .find(|(id, p)| {
                    id.starts_with(&format!("patch_operation:{kind}/"))
                        && p.controls.contains(&control)
                        && p.seeds().contains_key(slot)
                })
                .ok_or_else(|| anyhow::anyhow!("Offer not joined to ordinary controls"))?;
            ensure!(
                control.binding.kind == "invoke" && control.binding.presented,
                "Invalid offered action"
            );
            ensure!(
                offers
                    .insert(kind.into(), (body.0.clone(), control, slot.into()))
                    .is_none(),
                "Duplicate offer"
            );
        }
        for (name, value) in scene["offers"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Missing offers"))?
        {
            ensure!(
                ["moveNodes", "connect", "reconnect", "disconnect"].contains(&name.as_str()),
                "Unsupported offered patch operation {name}"
            );
            ensure!(
                value.as_str().is_some_and(|v| offers.contains_key(v)),
                "Unbound scene offer"
            );
        }
        Ok(Self {
            root,
            descriptor,
            scene,
            sequence,
            covered,
            bodies,
            references,
            offers,
        })
    }
    /// Occurrence lifetimes and reference subjects delimit editor ownership.
    pub fn compatible_body(&self, old: &Self, key: &str) -> bool {
        if !self.same_encounter(old) {
            return false;
        }
        let identity = |p: &Self| {
            p.descriptor["contents"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|c| {
                    let prefix = format!("{}:{}/", text(c, "kind"), text(c, "key"));
                    if !key.starts_with(&prefix) {
                        return None;
                    }
                    let collection = match text(c, "kind") {
                        "patch_node" => "nodes",
                        "patch_connection" => "connections",
                        _ => return Some((prefix.len(), Value::Null)),
                    };
                    p.scene[collection]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|n| n["key"] == c["key"])
                        .map(|n| (prefix.len(), n["lifetime"].clone()))
                })
                .max_by_key(|(len, _)| *len)
                .map(|(_, v)| v)
        };
        if identity(self) != identity(old) {
            return false;
        }
        if self.references.contains_key(key) || old.references.contains_key(key) {
            return self
                .references
                .get(key)
                .zip(old.references.get(key))
                .is_some_and(|(a, b)| a["target"] == b["target"] && a["slot"] == b["slot"]);
        }
        self.bodies.contains_key(key) && old.bodies.contains_key(key)
    }
    pub fn same_encounter(&self, other: &Self) -> bool {
        self.root.lifetime() == other.root.lifetime()
            && self.scene["header"]["surface"] == other.scene["header"]["surface"]
            && self.scene["header"]["encounter"] == other.scene["header"]["encounter"]
            && self.scene["header"]["rendererEpoch"] == other.scene["header"]["rendererEpoch"]
    }
    pub fn invocation(
        &self,
        kind: &str,
        args: Value,
        selections: &BTreeMap<String, SelectionCapture>,
    ) -> Result<Invocation> {
        ensure!(text(&args, "kind") == kind, "Gesture kind mismatch");
        let name = match kind {
            "move-nodes" => "moveNodes",
            "connect" => "connect",
            "reconnect" => "reconnect",
            "disconnect" => "disconnect",
            _ => bail!("Unsupported gesture"),
        };
        let offer = text(&self.scene["offers"], name);
        let (body, c, slot) = self
            .offers
            .get(offer)
            .ok_or_else(|| anyhow::anyhow!("Operation not offered"))?;
        self.check_args(&args)?;
        let p = &self.bodies[body];
        let mut inputs = p.seeds();
        inputs.insert(slot.clone(), serde_json::to_string(&args)?);
        Invocation::capture(p, c, inputs, selections)
    }
    pub fn check_args(&self, args: &Value) -> Result<()> {
        let ns = array(&self.scene, "nodes")?;
        let port = |t: &Value| {
            ns.iter().any(|n| {
                n["ports"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| port_target(n, p) == *t && p["connectable"] == true)
            })
        };
        match text(args, "kind") {
            "move-nodes" => {
                let mut seen = BTreeSet::new();
                let changes = array(args, "nodes")?;
                ensure!(
                    !changes.is_empty() && changes.len() <= 256,
                    "Invalid move selection"
                );
                for v in changes {
                    let n = ns
                        .iter()
                        .find(|n| node_target(n) == v["target"])
                        .ok_or_else(|| anyhow::anyhow!("Stale movement target"))?;
                    ensure!(
                        n["draggable"] == true
                            && seen.insert(portable_key(&v["target"]))
                            && v["baseOrigin"]["x"] == n["rect"]["x"]
                            && v["baseOrigin"]["y"] == n["rect"]["y"],
                        "Invalid movement basis"
                    );
                    number(&v["proposedOrigin"], "x")?;
                    number(&v["proposedOrigin"], "y")?;
                }
            }
            "connect" => ensure!(
                port(&args["from"]) && port(&args["to"]) && args["from"] != args["to"],
                "Stale connection ports"
            ),
            "disconnect" | "reconnect" => {
                let c = array(&self.scene, "connections")?
                    .iter()
                    .find(|c| connection_target(c) == args["connection"])
                    .ok_or_else(|| anyhow::anyhow!("Stale connection"))?;
                ensure!(c["topologyRef"] == args["topologyRef"], "Stale topology");
                if text(args, "kind") == "reconnect" {
                    ensure!(
                        c["reconnectable"] == true
                            && port(&args["to"])
                            && array(c, "endpoints")?
                                .iter()
                                .any(|e| e["key"] == args["endpoint"]),
                        "Invalid reconnect"
                    );
                } else {
                    ensure!(c["disconnectable"] == true, "Disconnect unavailable");
                }
            }
            _ => bail!("Unknown gesture"),
        }
        Ok(())
    }
}
pub fn portable_key(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}
use eframe::egui::{self, emath::TSTransform, *};
use std::sync::{Arc, Mutex, mpsc};
enum Event {
    Render(u64, Box<Result<Packet, String>>),
    Action(Uuid, String, Outcome),
    Embedded(String, Uuid, u64, Box<Result<Presentation, String>>),
}
#[derive(Clone)]
pub struct Work {
    pub id: Uuid,
    pub packet: Packet,
    pub editors: BTreeMap<String, Editor>,
    pub notice: String,
    pub selection: BTreeSet<String>,
    pub camera: TSTransform,
    fit_requested: bool,
    pub loading: bool,
    pub preparation_paused: bool,
    pub generation: u64,
    client: Option<Client>,
    events: Arc<Mutex<mpsc::Receiver<Event>>>,
    tx: mpsc::Sender<Event>,
    drag: Option<(Packet, Vec<Value>, Vec2)>,
    drag_pointer: Option<(Id, Pos2)>,
    drag_was_selected: bool,
    port: Option<(Packet, Value)>,
    reconnect: Option<Value>,
    pub inspection: Option<crate::inspector::Inspection>,
    pub changed: bool,
    overlay: Option<(String, Value)>,
    needs_refresh: bool,
    trace_enabled: bool,
    trace: std::collections::VecDeque<String>,
}
impl Work {
    pub fn new(packet: Packet) -> Result<Self> {
        let mut editors: BTreeMap<String, Editor> = packet
            .bodies
            .iter()
            .map(|(key, p)| {
                let mut e = Editor::new(p.snapshot.subject.clone(), p.view.clone());
                let t = e.request(p.context.clone()).unwrap();
                e.receive(t, p.clone())?;
                Ok((key.clone(), e))
            })
            .collect::<Result<_>>()?;
        for (key, reference) in &packet.references {
            editors.insert(
                key.clone(),
                Editor::new(
                    text(reference, "target").into(),
                    text(reference, "slot").into(),
                ),
            );
        }
        let initial = &packet.scene["initialCamera"];
        let fit_requested = initial["kind"] == "fit";
        let camera = if fit_requested {
            TSTransform::IDENTITY
        } else {
            TSTransform::new(
                vec2(number(initial, "tx")? as f32, number(initial, "ty")? as f32),
                number(initial, "scale")? as f32,
            )
        };
        let (tx, rx) = mpsc::channel();
        Ok(Self {
            id: Uuid::new_v4(),
            packet,
            editors,
            notice: "Patch ready".into(),
            selection: BTreeSet::new(),
            camera,
            fit_requested,
            loading: false,
            preparation_paused: false,
            generation: 0,
            client: None,
            events: Arc::new(Mutex::new(rx)),
            tx,
            drag: None,
            drag_pointer: None,
            drag_was_selected: false,
            port: None,
            reconnect: None,
            inspection: None,
            changed: false,
            overlay: None,
            needs_refresh: false,
            trace_enabled: false,
            trace: Default::default(),
        })
    }
    pub fn bind(&mut self, client: &Client) {
        self.client = Some(client.clone());
    }
    pub fn interacting(&self) -> bool {
        self.drag.is_some() || self.port.is_some() || self.reconnect.is_some()
    }
    pub fn can_leave(&self) -> bool {
        self.editors.values().all(|e| e.can_leave() && !e.dirty())
            && self.drag.is_none()
            && self.port.is_none()
            && self.reconnect.is_none()
    }
    pub fn ready(&self) -> bool {
        !self.loading && self.editors.values().all(Editor::can_act)
    }
    pub fn merge(&mut self, old: &Self) -> Result<()> {
        ensure!(
            !old.interacting(),
            "Finish or cancel the patch gesture before replacing its projection"
        );
        ensure!(
            self.packet.same_encounter(&old.packet),
            "Patch encounter replaced"
        );
        ensure!(
            self.packet.sequence >= old.packet.sequence,
            "Stale complete patch projection"
        );
        for (key, e) in &old.editors {
            ensure!(
                (self.editors.contains_key(key) && self.packet.compatible_body(&old.packet, key))
                    || (!e.dirty() && e.can_leave()),
                "Patch body retirement would lose work"
            );
        }
        for (key, e) in &mut self.editors {
            if let Some(previous) = old.editors.get(key) {
                if !self.packet.compatible_body(&old.packet, key) {
                    continue;
                }
                if let Some(reference) = self.packet.references.get(key) {
                    if old.packet.references.get(key).is_some_and(|old| {
                        old["target"] == reference["target"] && old["slot"] == reference["slot"]
                    }) {
                        *e = previous.clone();
                        if e.pending.is_none() {
                            e.requested = None;
                            e.stale = true;
                        }
                    } else {
                        ensure!(
                            previous.can_leave() && !previous.dirty(),
                            "Embedded subject replacement would lose work"
                        );
                    }
                    continue;
                }
                let p = e.presentation.clone().unwrap();
                *e = previous.clone();
                if e.pending.is_none() {
                    let suspended = e.suspended;
                    e.suspended = false;
                    let t = e.request(p.context.clone()).unwrap();
                    ensure!(
                        e.receive(t, p)?,
                        "Body preparation would replace working state; use the workspace departure guard"
                    );
                    if suspended {
                        e.suspend();
                    }
                }
            }
        }
        self.overlay = old
            .overlay
            .clone()
            .filter(|(id, _)| !self.packet.covered.contains(id));
        self.id = old.id;
        self.camera = old.camera;
        self.fit_requested = old.fit_requested;
        self.selection = old.selection.clone();
        self.events = old.events.clone();
        self.tx = old.tx.clone();
        self.client = old.client.clone();
        self.generation = old.generation;
        self.inspection = old.inspection.clone();
        self.trace_enabled = old.trace_enabled;
        self.trace = old.trace.clone();
        let mut live = BTreeSet::new();
        for n in self.packet.scene["nodes"].as_array().unwrap() {
            live.insert(portable_key(&node_target(n)));
            for p in n["ports"].as_array().unwrap() {
                live.insert(portable_key(&port_target(n, p)));
            }
        }
        for c in self.packet.scene["connections"].as_array().unwrap() {
            live.insert(portable_key(&connection_target(c)));
            for part in c["parts"].as_array().unwrap() {
                live.insert(portable_key(&json!({"kind":"connection-part","connection":c["key"],"connectionLifetime":c["lifetime"],"part":part["key"],"lifetime":part["lifetime"]})));
            }
        }
        self.selection.retain(|k| live.contains(k));
        Ok(())
    }
    pub fn refresh(&mut self, ctx: &egui::Context) {
        if self.loading
            || self
                .editors
                .values()
                .any(|e| e.pending.is_some() || e.composing)
        {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.loading = true;
        self.needs_refresh = false;
        self.generation += 1;
        self.drag = None;
        self.drag_pointer = None;
        self.port = None;
        self.reconnect = None;
        let ticket = self.generation;
        let p = self.packet.root.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        for e in self.editors.values_mut() {
            e.stale = true;
        }
        std::thread::spawn(move || {
            let result = client
                .render(&p.snapshot.subject, "view", &p.view, &p.context)
                .and_then(Packet::admit)
                .map_err(|e| e.to_string());
            let _ = tx.send(Event::Render(ticket, Box::new(result)));
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if !self.preparation_paused
            && let Some(client) = self.client.clone()
        {
            for (key, r) in &self.packet.references {
                let e = self.editors.get_mut(key).unwrap();
                if e.requested.is_none() && !e.composing && !e.suspended && e.pending.is_none() {
                    let width = rect(&r["rect"]).unwrap().width.max(1.);
                    let context = crate::model::Context {
                        width,
                        ..self.packet.root.context.clone()
                    };
                    let ticket = e.request(context.clone()).unwrap();
                    let owner = e.id;
                    let target = e.target.clone();
                    let slot = e.slot.clone();
                    let key = key.clone();
                    let tx = self.tx.clone();
                    let client = client.clone();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let p = client
                            .render(&target, "slot", &slot, &context)
                            .map_err(|e| e.to_string());
                        let _ = tx.send(Event::Embedded(key, owner, ticket, Box::new(p)));
                        ctx.request_repaint();
                    });
                }
            }
        }
        let events: Vec<_> = self.events.lock().unwrap().try_iter().collect();
        for event in events {
            if self.preparation_paused && matches!(event, Event::Embedded(..) | Event::Render(..)) {
                let _ = self.tx.send(event);
                continue;
            }
            match event {
                Event::Embedded(key, owner, ticket, result) => {
                    if let Some(e) = self.editors.get_mut(&key)
                        && e.id == owner
                        && e.generation == ticket
                        && !e.suspended
                    {
                        match *result {
                            Ok(p) => {
                                if p.controls.iter().any(|c| c.r#become) {
                                    e.status = "Embedded recipe transitions are unsupported".into();
                                    continue;
                                }
                                if let Err(error) = e.receive(ticket, p) {
                                    e.status = error.to_string();
                                }
                            }
                            Err(error) => e.status = format!("Embedded refresh failed: {error}"),
                        }
                    }
                }
                Event::Render(ticket, result) => {
                    if ticket != self.generation {
                        continue;
                    }
                    self.loading = false;
                    match *result {
                        Ok(packet) => {
                            let mut next = Self::new(packet).unwrap();
                            match next.merge(self) {
                                Ok(()) => {
                                    next.notice = "Patch refreshed from Reaver".into();
                                    *self = next;
                                }
                                Err(e) => self.notice = format!("Patch retained: {e}"),
                            }
                        }
                        Err(e) => self.notice = format!("Refresh failed; actions paused: {e}"),
                    }
                }
                Event::Action(owner, id, outcome) => {
                    if let Some(e) = self
                        .editors
                        .values_mut()
                        .find(|e| e.id == owner && e.pending.as_ref().is_some_and(|i| i.id() == id))
                    {
                        if matches!(outcome, Outcome::Rejected(_)) {
                            self.overlay = None;
                        }
                        let accepted = e.complete(&id, outcome);
                        self.notice = e.status.clone();
                        if accepted {
                            self.changed = true;
                            self.needs_refresh = true;
                            self.refresh(ctx);
                        }
                    }
                }
            }
        }
    }
    fn send(&mut self, owner: Uuid, i: Invocation, status: bool, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = if status {
                client.status(&i)
            } else {
                client.activate(&i)
            };
            let _ = tx.send(Event::Action(owner, i.id().into(), result));
            ctx.request_repaint();
        });
    }
    pub fn submit(&mut self, basis: &Packet, args: Value, ctx: &egui::Context) -> Result<()> {
        ensure!(
            self.ready()
                && self.packet.same_encounter(basis)
                && self.packet.sequence == basis.sequence,
            "Gesture superseded; refresh before retrying"
        );
        let kind = text(&args, "kind");
        let i = basis.invocation(kind, args.clone(), &BTreeMap::new())?;
        let offer = match kind {
            "move-nodes" => "moveNodes",
            v => v,
        };
        let (body, _, slot) = &basis.offers[text(&basis.scene["offers"], offer)];
        let e = self.editors.get_mut(body).unwrap();
        ensure!(
            !e.dirty(),
            "Operation form has a draft; apply or discard it first"
        );
        e.edit(slot, serde_json::to_string(&args)?);
        self.overlay = Some((i.id().to_owned(), args.clone()));
        e.pending = Some(i.clone());
        e.sending = true;
        e.revision += 1;
        e.generation += 1;
        let owner = e.id;
        for e in self.editors.values_mut() {
            e.stale = true;
        }
        self.notice = "Applying offered patch operation…".into();
        self.send(owner, i, false, ctx);
        Ok(())
    }
    fn move_args(nodes: &[Value], delta: Vec2) -> Value {
        json!({"kind":"move-nodes","nodes":nodes.iter().map(|n|json!({"target":node_target(n),"baseOrigin":{"x":n["rect"]["x"],"y":n["rect"]["y"]},"proposedOrigin":{"x":n["rect"]["x"].as_f64().unwrap()+delta.x as f64,"y":n["rect"]["y"].as_f64().unwrap()+delta.y as f64}})).collect::<Vec<_>>()})
    }
    fn choose(&mut self, target: Value, add: bool) {
        let key = portable_key(&target);
        if !add {
            self.selection.clear();
            self.selection.insert(key);
        } else if !self.selection.remove(&key) && self.selection.len() < 256 {
            self.selection.insert(key);
        }
    }
    fn port_click(&mut self, p: Value, ctx: &egui::Context) -> Result<()> {
        ensure!(self.ready(), "Patch is not fresh");
        if let Some(c) = self.reconnect.take() {
            let mut args = c;
            args["to"] = p;
            return self.submit(&self.packet.clone(), args, ctx);
        }
        if let Some((basis, from)) = self.port.take() {
            self.submit(&basis, json!({"kind":"connect","from":from,"to":p}), ctx)
        } else {
            self.port = Some((self.packet.clone(), p));
            self.notice = "Choose the destination port; Escape cancels".into();
            Ok(())
        }
    }
    pub fn activate_body(
        &mut self,
        key: &str,
        control: &Control,
        ctx: &egui::Context,
    ) -> Result<()> {
        let e = self
            .editors
            .get_mut(key)
            .ok_or_else(|| anyhow::anyhow!("Missing body owner"))?;
        let invocation = e.activate(control, &BTreeMap::new())?;
        let owner = e.id;
        for e in self.editors.values_mut() {
            e.stale = true;
        }
        self.send(owner, invocation, false, ctx);
        Ok(())
    }
    pub fn recover_body(&mut self, key: &str, status: bool, ctx: &egui::Context) -> Result<()> {
        let e = self
            .editors
            .get_mut(key)
            .ok_or_else(|| anyhow::anyhow!("Missing body owner"))?;
        ensure!(!e.sending, "Save is still in flight");
        let invocation = e
            .pending
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No unresolved invocation"))?;
        e.sending = true;
        let owner = e.id;
        self.send(owner, invocation, status, ctx);
        Ok(())
    }
    fn reference_ui(&mut self, world: &mut Ui, key: &str, origin: Pos2) -> Option<Control> {
        let b = rect(&self.packet.references[key]["rect"]).unwrap();
        let r = Rect::from_min_size(
            origin + vec2(b.x as f32, b.y as f32),
            vec2(b.width as f32, b.height as f32),
        );
        let e = self.editors.get_mut(key).unwrap();
        let mut action = None;
        world.scope_builder(UiBuilder::new().max_rect(r), |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(r));
            ScrollArea::vertical()
                .id_salt((e.id, "embedded-scroll"))
                .max_height(r.height())
                .show(ui, |ui| {
                    let (a, node) = crate::painter::surface(ui, e, &BTreeMap::new());
                    action = a;
                    if node.is_some() {
                        self.inspection = crate::inspector::Inspection::capture(e, node).ok();
                    }
                    if e.presentation.is_none() {
                        ui.label(&e.status);
                    }
                });
        });
        action
    }
    pub fn ui(&mut self, ui: &mut Ui) {
        self.poll(ui.ctx());
        if self.needs_refresh && !self.loading {
            self.refresh(ui.ctx());
        }
        let ctx = ui.ctx().clone();
        if self.trace_enabled {
            let events = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::PointerMoved(p) => Some(format!("move {p:?}")),
                        egui::Event::PointerButton {
                            pos,
                            button,
                            pressed,
                            ..
                        } => Some(format!("{button:?} {pressed} {pos:?}")),
                        egui::Event::WindowFocused(f) => Some(format!("focus {f}")),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            });
            if !events.is_empty() {
                self.trace.push_back(format!(
                    "ready={} drag={} {}",
                    self.ready(),
                    self.drag.is_some(),
                    events.join("; ")
                ));
            }
        }
        let mut gesture = None;
        let mut ordinary = None;
        let mut retry = None;
        ui.horizontal(|ui| {
            if ui.button("Refresh patch").clicked() {
                self.refresh(&ctx);
            }
            crate::inspector::pipeline_toggle(ui);
            if ui.button("Fit scene").clicked() {
                self.fit_requested = true;
            }
            if ui.button("Received patch Goo").clicked() {
                let mut owner = Editor::new(
                    self.packet.root.snapshot.subject.clone(),
                    self.packet.root.view.clone(),
                );
                owner.id = self.id;
                owner.mounted_generation = self.generation;
                owner.presentation = Some(self.packet.root.clone());
                self.inspection = crate::inspector::Inspection::capture(&owner, None).ok();
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = 90.;
            ui.add(Slider::new(&mut self.camera.scaling,
                number(&self.packet.scene["policy"], "minScale").unwrap() as f32..=
                number(&self.packet.scene["policy"], "maxScale").unwrap() as f32)
                .show_value(false).text("Scene zoom"));
            ui.label(RichText::new(format!("{:.0}%", self.camera.scaling * 100.)).small().color(crate::chrome::MUTED));
            ui.separator();
            ui.label(RichText::new("Gesture help").small().color(crate::chrome::MUTED))
                .on_hover_text("Drag a handle to move. Choose two ports to connect. Shift-click selects a group. Middle drag pans. Escape cancels.");
        });
        ui.label(
            RichText::new(&self.notice)
                .small()
                .color(crate::chrome::MUTED),
        );
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            self.drag = None;
            self.drag_pointer = None;
            self.port = None;
            self.reconnect = None;
            self.notice = "Gesture cancelled".into();
        }
        if self.drag.is_some() && (!self.ready() || !ui.input(|i| i.focused)) {
            self.drag = None;
            self.drag_pointer = None;
            self.notice = "Gesture cancelled; refresh before moving again".into();
        }
        let (canvas, response) =
            ui.allocate_exact_size(vec2(ui.available_width().max(100.), 420.), Sense::drag());
        ui.painter()
            .rect_filled(canvas, 8., crate::chrome::BACKGROUND);
        ui.painter().rect_stroke(
            canvas,
            8.,
            Stroke::new(1_f32, crate::chrome::BORDER),
            StrokeKind::Inside,
        );
        let extent = rect(&self.packet.scene["extent"]).unwrap();
        if self.fit_requested {
            self.fit_requested = false;
            let padding = self.packet.scene["initialCamera"]["paddingPx"]
                .as_f64()
                .unwrap_or(32.) as f32;
            let scale = ((canvas.width() - 2. * padding) / (extent.width as f32).max(1.))
                .min((canvas.height() - 2. * padding) / (extent.height as f32).max(1.))
                .clamp(
                    number(&self.packet.scene["policy"], "minScale").unwrap() as f32,
                    number(&self.packet.scene["policy"], "maxScale").unwrap() as f32,
                );
            self.camera = TSTransform::new(
                vec2(
                    padding - extent.x as f32 * scale,
                    padding - extent.y as f32 * scale,
                ),
                scale,
            );
        }
        if response.dragged_by(PointerButton::Middle) {
            self.camera.translation += ui.input(|i| i.pointer.delta());
        }
        if response.hovered() {
            let zoom = ui.input(|i| i.zoom_delta());
            if zoom != 1. {
                let old = self.camera.scaling;
                let new = (old * zoom).clamp(
                    number(&self.packet.scene["policy"], "minScale").unwrap() as f32,
                    number(&self.packet.scene["policy"], "maxScale").unwrap() as f32,
                );
                let mouse = ui
                    .input(|i| i.pointer.hover_pos())
                    .unwrap_or(canvas.center())
                    - canvas.min;
                self.camera.translation = mouse - (mouse - self.camera.translation) * (new / old);
                self.camera.scaling = new;
            }
        }
        if self.drag.is_some()
            && let Some(pointer) = ui.input(|i| i.pointer.hover_pos())
        {
            let band = number(&self.packet.scene["policy"], "autoPanBandPx").unwrap() as f32;
            let speed =
                number(&self.packet.scene["policy"], "autoPanMaxPxPerSecond").unwrap() as f32;
            if band > 0. {
                let mut velocity = Vec2::ZERO;
                for axis in 0..2 {
                    if pointer[axis] < canvas.min[axis] + band {
                        velocity[axis] =
                            ((canvas.min[axis] + band - pointer[axis]) / band).clamp(0., 1.);
                    } else if pointer[axis] > canvas.max[axis] - band {
                        velocity[axis] =
                            -((pointer[axis] - canvas.max[axis] + band) / band).clamp(0., 1.);
                    }
                }
                let pan = velocity * speed * ui.input(|i| i.stable_dt.min(0.05));
                self.camera.translation += pan;
                if let Some((_, _, delta)) = &mut self.drag {
                    *delta -= pan / self.camera.scaling;
                }
                if pan != Vec2::ZERO {
                    ctx.request_repaint();
                }
            }
        }
        let transform = TSTransform::new(
            canvas.min.to_vec2() + self.camera.translation,
            self.camera.scaling,
        );
        // A camera-aligned grid is host interaction chrome, not scene layout.
        let grid = 24. * self.camera.scaling.max(0.5);
        let origin = canvas.min + self.camera.translation;
        let mut x = canvas.left() + (origin.x - canvas.left()).rem_euclid(grid);
        let dots = ui.painter().with_clip_rect(canvas.shrink(2.));
        while x < canvas.right() {
            let mut y = canvas.top() + (origin.y - canvas.top()).rem_euclid(grid);
            while y < canvas.bottom() {
                dots.circle_filled(pos2(x, y), 0.7, crate::chrome::BORDER);
                y += grid;
            }
            x += grid;
        }
        let layer = LayerId::new(ui.layer_id().order, Id::new((self.id, "patch-world")));
        ui.ctx().set_transform_layer(layer, transform);
        ui.ctx().set_sublayer(ui.layer_id(), layer);
        let world_clip = transform.inverse() * canvas;
        let mut world = ui.new_child(UiBuilder::new().layer_id(layer).max_rect(world_clip));
        world.set_clip_rect(world_clip);
        let scene = self.packet.scene.clone();
        let contents = self.packet.descriptor["contents"]
            .as_array()
            .unwrap()
            .clone();
        let ready = self.ready();
        let color = ui.visuals().text_color();
        let point = |v: &Value| {
            pos2(
                v["x"].as_f64().unwrap() as f32,
                v["y"].as_f64().unwrap() as f32,
            )
        };
        for c in scene["connections"].as_array().unwrap() {
            for s in c["segments"].as_array().unwrap() {
                let mut from = Pos2::ZERO;
                let mut tangent = Vec2::X;
                let mut first_tangent = None;
                let mut first = None;
                let mut bounds = Rect::NOTHING;
                for cmd in s["commands"].as_array().unwrap() {
                    let to = point(&cmd["to"]);
                    match text(cmd, "op") {
                        "M" => {}
                        "L" => {
                            world
                                .painter()
                                .line_segment([from, to], Stroke::new(2_f32, color));
                        }
                        "C" => {
                            world
                                .painter()
                                .add(epaint::CubicBezierShape::from_points_stroke(
                                    [from, point(&cmd["c1"]), point(&cmd["c2"]), to],
                                    false,
                                    Color32::TRANSPARENT,
                                    Stroke::new(2_f32, color),
                                ));
                        }
                        _ => {}
                    }
                    if text(cmd, "op") != "M" {
                        first_tangent.get_or_insert(if text(cmd, "op") == "C" {
                            point(&cmd["c1"]) - from
                        } else {
                            to - from
                        });
                        tangent = to
                            - if text(cmd, "op") == "C" {
                                point(&cmd["c2"])
                            } else {
                                from
                            };
                    } else {
                        first = Some(to);
                    }
                    bounds.extend_with(to);
                    if text(cmd, "op") == "C" {
                        bounds.extend_with(point(&cmd["c1"]));
                        bounds.extend_with(point(&cmd["c2"]));
                    }
                    from = to;
                }
                for (marker, at, direction) in [
                    (
                        text(s, "startMarker"),
                        first.unwrap_or(from),
                        -first_tangent.unwrap_or(tangent),
                    ),
                    (text(s, "endMarker"), from, tangent),
                ] {
                    if marker == "dot" {
                        world.painter().circle_filled(at, 3., color);
                    } else if marker == "arrow" && direction.length_sq() > 0. {
                        let d = direction.normalized();
                        let side = vec2(-d.y, d.x);
                        world.painter().add(Shape::convex_polygon(
                            vec![at, at - d * 9. + side * 4., at - d * 9. - side * 4.],
                            color,
                            Stroke::NONE,
                        ));
                    }
                }
                let pick = world.interact(
                    bounds.expand(6.),
                    Id::new((self.id, text(c, "key"), text(s, "key"))),
                    Sense::click(),
                );
                if pick.clicked() {
                    let target = if let Some(part) = c["parts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| p["key"] == s["part"])
                    {
                        json!({"kind":"connection-part","connection":c["key"],"connectionLifetime":c["lifetime"],"part":part["key"],"lifetime":part["lifetime"]})
                    } else {
                        connection_target(c)
                    };
                    self.choose(target, world.input(|i| i.modifiers.shift));
                }
            }
            let selected = self
                .selection
                .contains(&portable_key(&connection_target(c)));
            if selected {
                for j in c["junctions"].as_array().unwrap() {
                    world.painter().circle_stroke(
                        point(&j["point"]),
                        5.,
                        Stroke::new(2_f32, color),
                    );
                }
            }
        }
        let mut ordered_nodes: Vec<_> = scene["nodes"].as_array().unwrap().iter().collect();
        ordered_nodes.sort_by(|a, b| {
            a["z"]
                .as_f64()
                .unwrap()
                .total_cmp(&b["z"].as_f64().unwrap())
        });
        for n in ordered_nodes {
            let key = text(n, "key");
            let b = rect(&n["rect"]).unwrap();
            let delta = self
                .drag
                .as_ref()
                .filter(|(_, nodes, _)| nodes.iter().any(|v| v["key"] == n["key"]))
                .map(|(_, _, delta)| *delta)
                .or_else(|| {
                    self.overlay
                        .as_ref()
                        .and_then(|(_, args)| args["nodes"].as_array())
                        .and_then(|nodes| nodes.iter().find(|v| v["target"] == node_target(n)))
                        .map(|v| point(&v["proposedOrigin"]) - point(&n["rect"]))
                })
                .unwrap_or_default();
            let r = Rect::from_min_size(
                pos2(b.x as f32, b.y as f32) + delta,
                vec2(b.width as f32, b.height as f32),
            );
            let selected = self.selection.contains(&portable_key(&node_target(n)));
            let card = Rect::from_min_max(r.min - vec2(0., 24.), r.max);
            world.painter().rect_filled(
                card.translate(vec2(0., 3.)),
                7.,
                Color32::from_black_alpha(9),
            );
            world.painter().rect_filled(card, 7., Color32::WHITE);
            world.painter().rect_stroke(
                card,
                7.,
                Stroke::new(1_f32, crate::chrome::BORDER),
                StrokeKind::Inside,
            );
            if selected {
                world.painter().rect_stroke(
                    card.expand(3.),
                    8.,
                    Stroke::new(2_f32, ui.visuals().selection.stroke.color),
                    StrokeKind::Outside,
                );
            }
            let content = contents
                .iter()
                .find(|c| text(c, "kind") == "patch_node" && text(c, "key") == key)
                .unwrap();
            let mut y = r.top();
            for item in content["content"].as_array().unwrap() {
                match text(item, "kind") {
                    "body" => {
                        let id = format!("patch_node:{key}/{}", text(item, "key"));
                        if let Some(e) = self.editors.get_mut(&id) {
                            let height = e
                                .presentation
                                .as_ref()
                                .unwrap()
                                .display
                                .as_ref()
                                .unwrap()
                                .height as f32;
                            world.scope_builder(
                                UiBuilder::new().max_rect(Rect::from_min_size(
                                    pos2(r.left(), y),
                                    vec2(r.width(), height),
                                )),
                                |ui| {
                                    let (a, node) =
                                        crate::painter::surface(ui, e, &BTreeMap::new());
                                    if let Some(c) = a {
                                        ordinary = Some((id.clone(), c));
                                    }
                                    if node.is_some() {
                                        self.inspection =
                                            crate::inspector::Inspection::capture(e, node).ok();
                                    }
                                },
                            );
                            y += height;
                        }
                    }
                    "handle" => {
                        let hr =
                            Rect::from_min_size(r.left_top() - vec2(0., 24.), vec2(r.width(), 22.));
                        let response = world.interact(
                            hr,
                            Id::new((self.id, key, text(item, "key"))),
                            if ready {
                                Sense::click_and_drag()
                            } else {
                                Sense::hover()
                            },
                        );
                        world.painter().rect_filled(
                            hr,
                            6.,
                            if selected {
                                Color32::from_rgb(215, 238, 234)
                            } else {
                                Color32::from_rgb(231, 237, 240)
                            },
                        );
                        if ready {
                            response.clone().on_hover_cursor(CursorIcon::Grab);
                        }
                        world.painter().text(
                            hr.left_center() + vec2(8., 0.),
                            Align2::LEFT_CENTER,
                            text(item, "label"),
                            FontId::proportional(12.),
                            color,
                        );
                        response.widget_info(|| {
                            WidgetInfo::labeled(WidgetType::Button, ready, text(item, "label"))
                        });
                        if self.trace_enabled
                            && (response.hovered()
                                || response.drag_started()
                                || response.dragged()
                                || response.drag_stopped())
                            && world.input(|i| !i.events.is_empty())
                        {
                            self.trace.push_back(format!(
                                "handle {key}: hover={} start={} drag={} stop={} delta={:?}",
                                response.hovered(),
                                response.drag_started(),
                                response.dragged(),
                                response.drag_stopped(),
                                response.drag_delta()
                            ));
                        }
                        let press = ctx
                            .input(|i| {
                                i.events.iter().find_map(|event| match event {
                                    egui::Event::PointerButton {
                                        pos,
                                        button: PointerButton::Primary,
                                        pressed: true,
                                        ..
                                    } => Some(*pos),
                                    _ => None,
                                })
                            })
                            .filter(|p| {
                                ready
                                    && world.is_enabled()
                                    && self.drag.is_none()
                                    && response.interact_rect.contains(transform.inverse() * *p)
                                    && ctx
                                        .layer_id_at(*p)
                                        .is_some_and(|top| top == layer || top == ui.layer_id())
                            });
                        if response.clicked()
                            && self.drag_pointer.is_none()
                            && (press.is_none() || n["draggable"] != true)
                        {
                            self.choose(node_target(n), world.input(|i| i.modifiers.shift));
                        }
                        if let Some(press) = press
                            && n["draggable"] == true
                        {
                            if self.trace_enabled {
                                self.trace.push_back(format!(
                                    "captured {key} at projection {}",
                                    self.packet.sequence
                                ));
                            }
                            self.drag_was_selected = selected;
                            if !selected {
                                self.choose(node_target(n), world.input(|i| i.modifiers.shift));
                            }
                            let nodes = scene["nodes"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|v| {
                                    v["draggable"] == true
                                        && self.selection.contains(&portable_key(&node_target(v)))
                                })
                                .cloned()
                                .collect();
                            self.drag = Some((self.packet.clone(), nodes, Vec2::ZERO));
                            self.drag_pointer = Some((response.id, press));
                        }
                        // Native integrations may coalesce the last movement and release.
                        // In that case egui never emits drag_started/drag_stopped. Retain
                        // the handle captured on press and consume its release position.
                        let owns_pointer =
                            self.drag_pointer.is_some_and(|(id, _)| id == response.id);
                        if owns_pointer && let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                            let (_, previous) = self.drag_pointer.as_mut().unwrap();
                            if let Some((_, _, d)) = &mut self.drag {
                                *d += (pos - *previous) / self.camera.scaling;
                            }
                            *previous = pos;
                        }
                        let released = owns_pointer
                            && ctx.input(|i| i.pointer.button_released(PointerButton::Primary));
                        if released {
                            if self.trace_enabled {
                                self.trace.push_back(format!(
                                    "released {key}: world delta {:?}",
                                    self.drag.as_ref().map(|(_, _, d)| d)
                                ));
                            }
                            self.drag_pointer = None;
                            if self.drag_was_selected
                                && world.input(|i| i.modifiers.shift)
                                && self.drag.as_ref().is_some_and(|(_, _, d)| {
                                    d.length()
                                        < number(&scene["policy"], "dragThresholdPx").unwrap()
                                            as f32
                                            / self.camera.scaling
                                })
                            {
                                self.choose(node_target(n), true);
                            }
                        }
                        if released
                            && let Some((basis, nodes, d)) = self.drag.take()
                            && d.length()
                                >= number(&scene["policy"], "dragThresholdPx").unwrap() as f32
                                    / self.camera.scaling
                        {
                            let d = snap_delta(&basis, &nodes, d, self.camera.scaling);
                            gesture = Some((basis, Self::move_args(&nodes, d)));
                        }
                    }
                    "presentation" => {
                        let key = format!("patch_node:{key}/ref:{}", text(item, "key"));
                        if let Some(c) = self.reference_ui(&mut world, &key, r.min) {
                            ordinary = Some((key, c));
                        }
                    }
                    "port" => {
                        let p = n["ports"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|p| p["key"] == item["key"])
                            .unwrap();
                        let hit = rect(&p["hitRect"]).unwrap();
                        let pr = Rect::from_min_size(
                            r.min + vec2(hit.x as f32, hit.y as f32),
                            vec2(hit.width as f32, hit.height as f32),
                        );
                        world.painter().circle_filled(
                            r.min + point(&p["anchor"]).to_vec2(),
                            4.,
                            color,
                        );
                        let response = world
                            .interact(
                                pr,
                                Id::new((self.id, key, text(item, "key"))),
                                if ready {
                                    Sense::click()
                                } else {
                                    Sense::hover()
                                },
                            )
                            .on_hover_text(text(item, "label"));
                        response.widget_info(|| {
                            WidgetInfo::labeled(WidgetType::Button, ready, text(item, "label"))
                        });
                        if response.clicked()
                            && let Err(e) = self.port_click(port_target(n, p), &ctx)
                        {
                            self.notice = e.to_string();
                        }
                    }
                    "select" if world.put(r, Button::new(text(item, "label"))).clicked() => {
                        self.choose(node_target(n), world.input(|i| i.modifiers.shift));
                    }
                    _ => {}
                }
            }
        }
        for connection in scene["connections"].as_array().unwrap() {
            for label in connection["labels"].as_array().unwrap() {
                let b = rect(&label["rect"]).unwrap();
                let prefix = format!(
                    "patch_connection:{}/label:{}/",
                    text(connection, "key"),
                    text(label, "key")
                );
                let mut y = b.y as f32;
                let label_content = contents
                    .iter()
                    .find(|v| {
                        text(v, "kind") == "patch_connection" && v["key"] == connection["key"]
                    })
                    .unwrap()["content"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|v| text(v, "kind") == "label" && v["key"] == label["key"])
                    .unwrap();
                for item in label_content["children"].as_array().unwrap() {
                    if text(item, "kind") == "presentation" {
                        let key = format!("{prefix}ref:{}", text(item, "key"));
                        if let Some(c) =
                            self.reference_ui(&mut world, &key, pos2(b.x as f32, b.y as f32))
                        {
                            ordinary = Some((key, c));
                        }
                        continue;
                    }
                    let key = format!("{prefix}{}", text(item, "key"));
                    if let Some(e) = self.editors.get_mut(&key) {
                        let height = e
                            .presentation
                            .as_ref()
                            .and_then(|p| p.display.as_ref())
                            .map(|d| d.height as f32)
                            .unwrap_or(0.);
                        world.scope_builder(
                            UiBuilder::new().max_rect(Rect::from_min_size(
                                pos2(b.x as f32, y),
                                vec2(b.width as f32, height),
                            )),
                            |ui| {
                                let (a, node) = crate::painter::surface(ui, e, &BTreeMap::new());
                                if let Some(c) = a {
                                    ordinary = Some((key.clone(), c));
                                }
                                if node.is_some() {
                                    self.inspection =
                                        crate::inspector::Inspection::capture(e, node).ok();
                                }
                            },
                        );
                        y += height;
                    }
                }
            }
        }
        ui.collapsing("Accessible scene controls",|ui|{
            for n in scene["nodes"].as_array().unwrap(){ui.horizontal_wrapped(|ui|{if ui.selectable_label(self.selection.contains(&portable_key(&node_target(n))),text(n,"key")).clicked(){self.choose(node_target(n),ui.input(|i|i.modifiers.shift));}let content=contents.iter().find(|c|text(c,"kind")=="patch_node"&&c["key"]==n["key"]).unwrap();for item in content["content"].as_array().unwrap(){if text(item,"kind")=="handle"{for m in item["moves"].as_array().unwrap(){if ui.add_enabled(ready,Button::new(text(m,"label"))).clicked(){gesture=Some((self.packet.clone(),Self::move_args(std::slice::from_ref(n),vec2(number(m,"dx").unwrap() as f32,number(m,"dy").unwrap() as f32))));}}}
if text(item,"kind")=="port"{let p=n["ports"].as_array().unwrap().iter().find(|p|p["key"]==item["key"]).unwrap();if ui.add_enabled(ready,Button::new(text(item,"label"))).clicked()&&let Err(e)=self.port_click(port_target(n,p),&ctx){self.notice=e.to_string();}}}});}
            for group in self.packet.descriptor["connectionControls"].as_array().unwrap().clone(){let c=scene["connections"].as_array().unwrap().iter().find(|c|c["key"]==group["key"]).unwrap();ui.horizontal(|ui|{if ui.selectable_label(self.selection.contains(&portable_key(&connection_target(c))),text(c,"key")).clicked(){self.choose(connection_target(c),ui.input(|i|i.modifiers.shift));}for control in group["controls"].as_array().unwrap(){if ui.add_enabled(ready,Button::new(text(control,"label"))).clicked(){let args=json!({"kind":control["kind"],"connection":connection_target(c),"topologyRef":c["topologyRef"],"endpoint":control["endpoint"]});if text(control,"kind")=="disconnect"{gesture=Some((self.packet.clone(),args));}else{self.reconnect=Some(args);self.notice="Choose a new endpoint port; Escape cancels".into();}}}});}
        });
        for (key, e) in &mut self.editors {
            if !key.starts_with("patch_operation:") && e.pending.is_some() {
                ui.group(|ui| {
                    ui.label(format!("{} · {}", e.target, e.status));
                    let i = e.pending.clone().unwrap();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!e.sending, Button::new("Check embedded save status"))
                            .clicked()
                        {
                            retry = Some((e.id, i.clone(), true));
                        }
                        if ui
                            .add_enabled(!e.sending, Button::new("Retry exact embedded save"))
                            .clicked()
                        {
                            retry = Some((e.id, i, false));
                        }
                    });
                });
            }
            if key.starts_with("patch_operation:") {
                let label = key
                    .strip_prefix("patch_operation:")
                    .unwrap_or(key)
                    .split('/')
                    .next()
                    .unwrap()
                    .replace('-', " ");
                ui.collapsing(label, |ui| {
                    ui.small(&e.status);
                    if let Some(i) = e.pending.clone() {
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!e.sending, Button::new("Check operation status"))
                                .clicked()
                            {
                                retry = Some((e.id, i.clone(), true));
                            }
                            if ui
                                .add_enabled(!e.sending, Button::new("Retry exact operation"))
                                .clicked()
                            {
                                retry = Some((e.id, i, false));
                            }
                        });
                    }
                    if e.dirty() && e.can_leave() && ui.button("Discard operation draft").clicked()
                    {
                        for f in e.fields.values_mut() {
                            f.value = f.seed.clone();
                        }
                        e.revision += 1;
                    }
                    let (a, node) = crate::painter::surface(ui, e, &BTreeMap::new());
                    if let Some(c) = a {
                        ordinary = Some((key.clone(), c));
                    }
                    if node.is_some() {
                        self.inspection = crate::inspector::Inspection::capture(e, node).ok();
                    }
                });
            }
        }
        if let Some((basis, args)) = gesture
            && let Err(e) = self.submit(&basis, args, &ctx)
        {
            self.notice = e.to_string();
        }
        if let Some((key, c)) = ordinary
            && let Err(e) = self.activate_body(&key, &c, &ctx)
        {
            self.notice = e.to_string();
        }
        if let Some((owner, i, status)) = retry {
            if let Some(e) = self.editors.values_mut().find(|e| e.id == owner) {
                e.sending = true;
            }
            self.send(owner, i, status, &ctx);
        }
        if let Some(inspect) = &mut self.inspection {
            let mut open = true;
            inspect.show(&ctx, &mut open);
            if !open {
                self.inspection = None;
            }
        }
        if self.loading {
            ui.spinner();
        }
        while self.trace.len() > 32 {
            self.trace.pop_front();
        }
        ui.collapsing("Gesture diagnostics", |ui| {
            ui.checkbox(&mut self.trace_enabled, "Record pointer events");
            ui.small("Last 32 pointer/gesture records; no input text or action bodies.");
            ui.horizontal(|ui| {
                if ui.button("Clear trace").clicked() {
                    self.trace.clear();
                }
                if ui.button("Copy trace").clicked() {
                    ctx.copy_text(self.trace.iter().cloned().collect::<Vec<_>>().join("\n"));
                }
            });
            for entry in &self.trace {
                ui.monospace(entry);
            }
        });
    }
}

/// Apply the declared grid and alignment policy in logical screen pixels.
pub fn snap_delta(packet: &Packet, moving: &[Value], mut delta: Vec2, scale: f32) -> Vec2 {
    let Some(first) = moving.first() else {
        return delta;
    };
    let b = rect(&first["rect"]).unwrap();
    let policy = &packet.scene["policy"];
    let tolerance = number(policy, "snapTolerancePx").unwrap() as f32 / scale;
    if let Some(grid) = policy.get("grid") {
        for (axis, origin) in [(0, b.x as f32), (1, b.y as f32)] {
            let key = if axis == 0 { "x" } else { "y" };
            let step = number(&grid["step"], key).unwrap() as f32;
            let start = number(&grid["origin"], key).unwrap() as f32;
            let current = origin + delta[axis];
            let snapped = start + ((current - start) / step).round() * step;
            if (snapped - current).abs() <= tolerance {
                delta[axis] += snapped - current;
            }
        }
    }
    if policy["alignmentSnap"] == true {
        let mut best = [tolerance + 1.; 2];
        for n in packet.scene["nodes"].as_array().unwrap() {
            if moving.iter().any(|m| m["key"] == n["key"]) {
                continue;
            }
            let other = rect(&n["rect"]).unwrap();
            for source in moving {
                let a = rect(&source["rect"]).unwrap();
                for (axis, origin, size, target, span) in [
                    (0, a.x, a.width, other.x, other.width),
                    (1, a.y, a.height, other.y, other.height),
                ] {
                    for offset in [0., 0.5, 1.] {
                        let correction =
                            (target + span * offset - origin - size * offset) as f32 - delta[axis];
                        if correction.abs() <= tolerance && correction.abs() < best[axis].abs() {
                            best[axis] = correction;
                        }
                    }
                }
            }
        }
        for axis in 0..2 {
            if best[axis].abs() <= tolerance {
                delta[axis] += best[axis];
            }
        }
    }
    delta
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    fn packet() -> Packet {
        Packet::admit(
            Presentation::decode(include_bytes!("../tests/fixtures/patch-board.json")).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn stale_completion_cannot_clear_pending_overlay_and_coverage_retires_it() {
        let p = packet();
        let n = &p.scene["nodes"][0];
        let args = Work::move_args(std::slice::from_ref(n), vec2(10., 0.));
        let i = p
            .invocation("move-nodes", args.clone(), &BTreeMap::new())
            .unwrap();
        let mut work = Work::new(p).unwrap();
        let key = work.packet.offers["move-nodes"].0.clone();
        let owner = work.editors[&key].id;
        work.editors.get_mut(&key).unwrap().pending = Some(i.clone());
        work.overlay = Some((i.id().into(), args));
        let ctx = egui::Context::default();
        work.tx
            .send(Event::Action(
                owner,
                "old-invocation".into(),
                Outcome::Rejected("late".into()),
            ))
            .unwrap();
        work.poll(&ctx);
        assert!(work.overlay.is_some());
        assert_eq!(work.editors[&key].pending.as_ref().unwrap().id(), i.id());
        work.tx
            .send(Event::Action(
                owner,
                i.id().into(),
                Outcome::Unknown("timeout".into()),
            ))
            .unwrap();
        work.poll(&ctx);
        assert!(!work.can_leave());
        work.tx
            .send(Event::Action(owner, i.id().into(), Outcome::Accepted))
            .unwrap();
        work.poll(&ctx);
        assert!(work.overlay.is_some());
        assert!(work.needs_refresh);
        let mut uncovered = Work::new(packet()).unwrap();
        uncovered.merge(&work).unwrap();
        assert!(uncovered.overlay.is_some());
        let mut p = packet();
        p.covered.insert(i.id().into());
        let mut covered = Work::new(p).unwrap();
        covered.merge(&work).unwrap();
        assert!(covered.overlay.is_none());
    }
    #[test]
    fn embedded_inputs_follow_occurrence_position_and_preview_without_recreating_owner() {
        let mut raw: Value =
            serde_json::from_slice(include_bytes!("../tests/fixtures/patch-board.json")).unwrap();
        let view =
            Presentation::decode(include_bytes!("../tests/fixtures/live-view.json")).unwrap();
        let reference = json!({"kind":"presentation","key":"note","label":"Note","target":view.snapshot.subject,"slot":"/sys/slot/content","units":"css-px","rect":{"x":16,"y":0,"width":view.context.width,"height":800}});
        raw["description"][0]["children"][0]["contents"][0]["content"][1] = reference;
        let packet =
            Packet::admit(Presentation::decode(&serde_json::to_vec(&raw).unwrap()).unwrap())
                .unwrap();
        let mut work = Work::new(packet).unwrap();
        let key = "patch_node:source/ref:note";
        let e = work.editors.get_mut(key).unwrap();
        let ticket = e.request(view.context.clone()).unwrap();
        e.receive(ticket, view).unwrap();
        let owner = e.id;
        let slot = e.fields.keys().next().unwrap().clone();
        e.edit(&slot, "draft retained while moving".into());
        let ctx = egui::Context::default();
        crate::painter::install(&ctx);
        let frame = |work: &mut Work| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400., 1100.))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| work.ui(ui));
                },
            );
        };
        frame(&mut work);
        frame(&mut work);
        let before = ctx.read_response(Id::new((owner, &slot))).unwrap().rect;
        let source = work.packet.scene["nodes"][0].clone();
        assert!(before.min.x >= source["rect"]["x"].as_f64().unwrap() as f32 + 16.);
        let args = Work::move_args(&[source], vec2(50., 30.));
        work.overlay = Some(("pending-preview".into(), args));
        frame(&mut work);
        frame(&mut work);
        let after = ctx.read_response(Id::new((owner, &slot))).unwrap().rect;
        assert!((after.min - before.min - vec2(50., 30.)).length() < 0.1);
        assert_eq!(work.editors[key].id, owner);
        assert_eq!(
            work.editors[key].fields[&slot].value,
            "draft retained while moving"
        );
    }
    #[test]
    fn workspace_stage_defers_preparation_but_not_save_completion() {
        let mut work = Work::new(packet()).unwrap();
        work.preparation_paused = true;
        work.generation = 7;
        work.loading = true;
        work.tx
            .send(Event::Render(7, Box::new(Ok(packet()))))
            .unwrap();
        let node = work.packet.scene["nodes"][0].clone();
        let invocation = work
            .packet
            .invocation(
                "move-nodes",
                Work::move_args(&[node], vec2(10., 0.)),
                &BTreeMap::new(),
            )
            .unwrap();
        let key = work.packet.offers["move-nodes"].0.clone();
        let e = work.editors.get_mut(&key).unwrap();
        let id = e.id;
        e.pending = Some(invocation.clone());
        work.tx
            .send(Event::Action(
                id,
                invocation.id().into(),
                Outcome::Unknown("lost reply".into()),
            ))
            .unwrap();
        let ctx = egui::Context::default();
        work.poll(&ctx);
        assert!(
            work.loading,
            "preparation must wait for the staged workspace"
        );
        assert!(
            work.editors[&key].status.contains("Unknown"),
            "save outcome must still reach its owner"
        );
        work.preparation_paused = false;
        work.poll(&ctx);
        assert!(!work.loading);
        assert_eq!(work.editors[&key].id, id);
        assert_eq!(
            work.editors[&key].pending.as_ref().unwrap().body(),
            invocation.body()
        );
    }
    #[test]
    fn embedded_response_for_retired_owner_is_ignored() {
        let mut p = packet();
        let reference = json!({"target":"/0x11/app/ios_demo/note","slot":"/sys/slot/content","units":"css-px","rect":{"x":0,"y":0,"width":700,"height":200}});
        p.references
            .insert("patch_node:source/ref:note".into(), reference);
        let mut work = Work::new(p.clone()).unwrap();
        let key = "patch_node:source/ref:note";
        let retired = work.editors[key].id;
        p.references.get_mut(key).unwrap()["target"] = json!("/0x11/app/ios_demo/notes/b");
        let mut next = Work::new(p).unwrap();
        next.merge(&work).unwrap();
        assert_ne!(next.editors[key].id, retired);
        let response =
            Presentation::decode(include_bytes!("../tests/fixtures/live-view.json")).unwrap();
        work.tx
            .send(Event::Embedded(
                key.into(),
                retired,
                0,
                Box::new(Ok(response)),
            ))
            .unwrap();
        next.poll(&egui::Context::default());
        assert!(next.editors[key].presentation.is_none());
        assert!(!next.editors[key].can_act());
        let e = next.editors.get_mut(key).unwrap();
        e.generation = 10;
        e.status = "new request remains authoritative".into();
        let owner = e.id;
        next.tx
            .send(Event::Embedded(
                key.into(),
                owner,
                9,
                Box::new(Err("late failure".into())),
            ))
            .unwrap();
        next.poll(&egui::Context::default());
        assert_eq!(
            next.editors[key].status,
            "new request remains authoritative"
        );
        work.loading = true;
        work.generation = 4;
        work.tx
            .send(Event::Render(3, Box::new(Ok(packet()))))
            .unwrap();
        work.poll(&egui::Context::default());
        assert!(work.loading);
    }
}

#[cfg(test)]
mod gesture_tests {
    use super::*;
    #[test]
    fn pointer_drag_accumulates_and_freezes_the_offered_move() {
        exercise_drag(false, 0);
    }
    #[test]
    fn pointer_drag_inside_explorer_scrolling_surface() {
        exercise_drag(true, 0);
    }
    #[test]
    fn native_movement_and_release_in_one_frame_still_submit() {
        exercise_drag(true, 1);
    }
    #[test]
    fn native_entire_drag_in_one_frame_still_submits() {
        exercise_drag(true, 2);
    }
    #[test]
    fn escape_cancels_capture_without_invoking() {
        exercise_drag(true, 3);
    }
    #[test]
    fn stale_or_suspended_editor_cancels_capture_without_invoking() {
        exercise_drag(true, 4);
    }
    fn exercise_drag(in_scroll: bool, coalesced: u8) {
        let p = Packet::admit(
            Presentation::decode(include_bytes!("../tests/fixtures/patch-board.json")).unwrap(),
        )
        .unwrap();
        let mut work = Work::new(p).unwrap();
        let ctx = egui::Context::default();
        crate::painter::install(&ctx);
        let frame = |work: &mut Work, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 900.))),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if in_scroll {
                        ui.columns(2, |columns| {
                            ScrollArea::both()
                                .max_height(650.)
                                .show(&mut columns[1], |ui| work.ui(ui));
                        });
                    } else {
                        work.ui(ui);
                    }
                });
            });
        };
        frame(&mut work, vec![]);
        frame(&mut work, vec![]);
        let response = ctx
            .read_response(Id::new((work.id, "source", "move")))
            .unwrap();
        let to_global = ctx.layer_transform_to_global(response.layer_id).unwrap();
        let start = to_global * response.rect.center();
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        if coalesced != 2 {
            frame(
                &mut work,
                vec![egui::Event::PointerMoved(start), pointer(start, true)],
            );
        }
        if coalesced >= 3 {
            assert!(work.interacting());
            let mut replacement = Work::new(work.packet.clone()).unwrap();
            assert!(
                replacement.merge(&work).is_err(),
                "refresh cannot silently replace an active gesture"
            );
            assert!(work.interacting());
            if coalesced == 4 {
                for e in work.editors.values_mut() {
                    e.suspend();
                }
            }
            frame(
                &mut work,
                if coalesced == 3 {
                    vec![egui::Event::Key {
                        key: Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }]
                } else {
                    vec![]
                },
            );
            frame(
                &mut work,
                vec![
                    egui::Event::PointerMoved(start + vec2(32., 0.)),
                    pointer(start + vec2(32., 0.), false),
                ],
            );
            assert!(!work.interacting());
            assert!(work.editors.values().all(|e| e.pending.is_none()));
            return;
        }
        if coalesced > 0 {
            frame(&mut work, {
                let mut events = vec![];
                if coalesced == 2 {
                    events.extend([egui::Event::PointerMoved(start), pointer(start, true)]);
                }
                events.extend([
                    egui::Event::PointerMoved(start + vec2(32., 0.)),
                    pointer(start + vec2(32., 0.), false),
                ]);
                events
            });
            assert!(work.drag.is_none());
            assert!(
                work.editors[&work.packet.offers["move-nodes"].0]
                    .pending
                    .is_some(),
                "coalesced release must freeze an invocation: {}",
                work.notice
            );
            return;
        }
        frame(
            &mut work,
            vec![egui::Event::PointerMoved(start + vec2(12., 0.))],
        );
        assert!(work.drag.is_some(), "handle must own the drag");
        frame(
            &mut work,
            vec![egui::Event::PointerMoved(start + vec2(32., 0.))],
        );
        let delta = work.drag.as_ref().unwrap().2;
        assert!(
            (delta.x - 32. / to_global.scaling).abs() < 0.1,
            "full movement, not just the last frame: {delta:?}"
        );
        frame(&mut work, vec![pointer(start + vec2(32., 0.), false)]);
        assert!(work.drag.is_none());
        let key = &work.packet.offers["move-nodes"].0;
        assert!(
            work.editors[key].pending.is_some(),
            "release freezes a real offered invocation: {}",
            work.notice
        );
        assert!(work.overlay.is_some());
    }
}
