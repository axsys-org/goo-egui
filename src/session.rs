//! Session state is independent of widgets and worker threads. All completions carry incarnation/ticket.
use crate::{model::*, transport::*};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use uuid::Uuid;
#[derive(Clone, Debug)]
pub struct Field {
    pub seed: String,
    pub value: String,
}
impl Field {
    pub fn dirty(&self) -> bool {
        self.seed != self.value
    }
}
#[derive(Clone, Debug)]
pub struct Editor {
    pub id: Uuid,
    pub target: String,
    pub slot: String,
    pub member: Option<Member>,
    pub publication: Option<String>,
    pub presentation: Option<Presentation>,
    pub fields: BTreeMap<String, Field>,
    pub pending: Option<Invocation>,
    pub sending: bool,
    pub suspended: bool,
    pub composing: bool,
    pub stale: bool,
    pub generation: u64,
    pub mounted_generation: u64,
    pub revision: u64,
    pub requested: Option<Context>,
    pub status: String,
    pub candidate: Option<Presentation>,
}
impl Editor {
    pub fn new(target: String, slot: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            target,
            slot,
            member: None,
            publication: None,
            presentation: None,
            fields: BTreeMap::new(),
            pending: None,
            sending: false,
            suspended: false,
            composing: false,
            stale: true,
            generation: 0,
            mounted_generation: 0,
            revision: 0,
            requested: None,
            status: "Waiting for prepared view".into(),
            candidate: None,
        }
    }
    pub fn dirty(&self) -> bool {
        self.fields.values().any(Field::dirty)
    }
    pub fn can_leave(&self) -> bool {
        self.pending.is_none() && !self.composing
    }
    pub fn can_act(&self) -> bool {
        !self.stale
            && !self.suspended
            && !self.composing
            && self.pending.is_none()
            && self.presentation.is_some()
    }
    pub fn values(&self) -> BTreeMap<String, String> {
        self.fields
            .iter()
            .map(|(k, f)| (k.clone(), f.value.clone()))
            .collect()
    }
    pub fn edit(&mut self, slot: &str, value: String) {
        if let Some(f) = self.fields.get_mut(slot) {
            f.value = value;
            self.revision += 1;
        }
    }
    pub fn request(&mut self, context: Context) -> Option<u64> {
        if self.suspended || self.pending.is_some() {
            return None;
        }
        self.generation += 1;
        self.requested = Some(context);
        self.stale = true;
        self.candidate = None;
        self.status = "Refreshing; actions paused".into();
        Some(self.generation)
    }
    pub fn receive(&mut self, ticket: u64, p: Presentation) -> Result<bool> {
        if ticket != self.generation || self.suspended {
            return Ok(false);
        }
        p.validate(self.requested.as_ref().unwrap(), &self.target)?;
        let replacing = self
            .presentation
            .as_ref()
            .is_some_and(|old| old.lifetime() != p.lifetime())
            || self
                .member
                .as_ref()
                .is_some_and(|m| m.life != p.snapshot.life)
            || self
                .publication
                .as_ref()
                .is_some_and(|v| *v != p.snapshot.publication);
        let seeds = p.seeds();
        let loses = self
            .fields
            .iter()
            .any(|(k, f)| f.dirty() && !seeds.contains_key(k));
        if self.composing || ((replacing || loses) && self.dirty()) {
            self.candidate = Some(p);
            self.status = "Prepared replacement staged; draft retained".into();
            return Ok(false);
        }
        ensure!(
            !self
                .member
                .as_ref()
                .is_some_and(|m| m.life != p.snapshot.life),
            "Selected member replaced; reload declared tabs"
        );
        ensure!(
            !self
                .publication
                .as_ref()
                .is_some_and(|v| *v != p.snapshot.publication),
            "Publication replaced; reload declared tabs"
        );
        if replacing {
            self.fields.clear();
            self.id = Uuid::new_v4();
        }
        self.mount(p);
        Ok(true)
    }
    fn mount(&mut self, p: Presentation) {
        self.fields = p
            .seeds()
            .into_iter()
            .map(|(k, seed)| {
                let value = self
                    .fields
                    .get(&k)
                    .filter(|f| f.dirty())
                    .map_or_else(|| seed.clone(), |f| f.value.clone());
                (k, Field { seed, value })
            })
            .collect();
        self.presentation = Some(p);
        self.mounted_generation = self.generation;
        self.candidate = None;
        self.stale = false;
        self.revision += 1;
        self.status = if self.dirty() {
            "Draft retained"
        } else {
            "Live · persisted reread"
        }
        .into();
    }
    pub fn accept_candidate(&mut self, revision: u64) -> Result<()> {
        ensure!(
            self.can_leave() && self.revision == revision,
            "Replacement superseded by new work"
        );
        let p = self
            .candidate
            .take()
            .ok_or_else(|| anyhow::anyhow!("Replacement superseded"))?;
        ensure!(
            self.member
                .as_ref()
                .is_none_or(|m| m.life == p.snapshot.life)
                && self
                    .publication
                    .as_ref()
                    .is_none_or(|v| *v == p.snapshot.publication),
            "Reload deck to replace membership/publication"
        );
        self.fields.clear();
        self.id = Uuid::new_v4();
        self.mount(p);
        Ok(())
    }
    pub fn activate(
        &mut self,
        c: &Control,
        selections: &BTreeMap<String, SelectionCapture>,
    ) -> Result<Invocation> {
        ensure!(self.can_act(), "Editor is not actionable");
        let i = Invocation::capture(
            self.presentation.as_ref().unwrap(),
            c,
            self.values(),
            selections,
        )?;
        self.pending = Some(i.clone());
        self.sending = true;
        self.generation += 1;
        self.revision += 1;
        self.status = "Saving; edits and tab switching remain available".into();
        Ok(i)
    }
    pub fn complete(&mut self, id: &str, outcome: Outcome) -> bool {
        if self.pending.as_ref().is_none_or(|i| i.id() != id) {
            return false;
        }
        self.sending = false;
        self.revision += 1;
        match outcome {
            Outcome::Accepted => {
                let i = self.pending.take().unwrap();
                for (k, v) in i.inputs() {
                    if let Some(f) = self.fields.get_mut(k) {
                        f.seed = v.clone();
                    }
                }
                self.stale = true;
                self.status = "Save accepted; reread required".into();
                true
            }
            Outcome::Rejected(e) => {
                self.pending = None;
                self.stale = true;
                self.status = format!("Rejected; draft retained. Refresh before saving. {e}");
                false
            }
            Outcome::Unknown(e) => {
                self.status =
                    format!("Unknown save outcome. Check status or retry exact save. {e}");
                false
            }
        }
    }
    pub fn suspend(&mut self) -> bool {
        if self.composing {
            return false;
        }
        self.suspended = true;
        self.stale = true;
        self.generation += 1;
        self.candidate = None;
        true
    }
    pub fn resume(&mut self) {
        self.generation += 1;
        self.suspended = false;
        self.stale = true;
        self.requested = None;
    }
}
#[derive(Clone, Debug)]
pub struct Pane {
    pub id: Uuid,
    pub tab: Tab,
    pub source: Source,
    pub selected: Option<Member>,
    pub selection_id: String,
    pub selection_revision: u64,
    pub editor: Option<Editor>,
}
impl Pane {
    fn new(tab: Tab, source: Source) -> Result<Self> {
        source.validate(&tab)?;
        let selected = tab
            .initial
            .as_ref()
            .map(|key| {
                source
                    .members
                    .iter()
                    .find(|m| m.key == *key && m.status == "available")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Invalid creation seed"))
            })
            .transpose()?;
        Ok(Self {
            id: Uuid::new_v4(),
            tab,
            source,
            selected,
            selection_id: Uuid::new_v4().to_string(),
            selection_revision: 0,
            editor: None,
        })
    }
    pub fn captures(&self) -> BTreeMap<String, SelectionCapture> {
        self.selected
            .as_ref()
            .map(|m| {
                (
                    self.tab.selection.clone(),
                    SelectionCapture {
                        selection: self.selection_id.clone(),
                        incarnation: 1,
                        revision: self.selection_revision,
                        member: m.clone(),
                    },
                )
            })
            .into_iter()
            .collect()
    }
    pub fn mount_editor(&mut self, publication: &str) {
        if self.editor.is_some() {
            return;
        }
        if let Some(m) = &self.selected {
            let mut e = Editor::new(m.subject.clone(), self.tab.detail_slot.clone());
            e.member = Some(m.clone());
            e.publication = Some(publication.into());
            self.editor = Some(e);
        }
    }
}
#[derive(Clone)]
pub enum Change {
    Deck {
        tabs: Vec<Tab>,
        sources: BTreeMap<String, Source>,
        basis: String,
        publication: String,
    },
    Select {
        pane: Uuid,
        member: Member,
    },
    Remove {
        pane: Uuid,
    },
}
#[derive(Clone)]
pub struct Plan {
    pub ticket: u64,
    pub change: Change,
    pub affected: Vec<(Uuid, Option<(Uuid, u64)>)>,
}
#[derive(Default)]
pub struct Session {
    pub panes: Vec<Pane>,
    pub active: Option<Uuid>,
    pub basis: String,
    pub publication: String,
    pub intent: u64,
}
impl Session {
    pub fn active_mut(&mut self) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| Some(p.id) == self.active)
    }
    pub fn editor_mut(&mut self, id: Uuid) -> Option<&mut Editor> {
        self.panes
            .iter_mut()
            .filter_map(|p| p.editor.as_mut())
            .find(|e| e.id == id)
    }
    pub fn switch(&mut self, id: Uuid) -> bool {
        if !self.panes.iter().any(|p| p.id == id) {
            return false;
        }
        if self.active == Some(id) {
            return true;
        }
        if self
            .active_mut()
            .and_then(|p| p.editor.as_mut())
            .is_some_and(|e| !e.suspend())
        {
            return false;
        }
        self.active = Some(id);
        if let Some(e) = self.active_mut().and_then(|p| p.editor.as_mut()) {
            e.resume()
        }
        true
    }
    fn retained(&self, p: &Pane, t: &Tab, s: &Source, basis: &str) -> bool {
        let mut a = p.tab.clone();
        let mut b = t.clone();
        a.title.clear();
        b.title.clear();
        a.initial = None;
        b.initial = None;
        self.basis == basis && a == b && p.source.life == s.life
    }
    pub fn stage(&mut self, change: Change) -> Result<Plan> {
        self.intent += 1;
        let mut affected = vec![];
        match &change {
            Change::Deck {
                tabs,
                sources,
                basis,
                ..
            } => {
                let mut keys = std::collections::BTreeSet::new();
                ensure!(
                    tabs.len() <= 8 && tabs.iter().all(|t| keys.insert(&t.id)),
                    "Duplicate/excessive panes"
                );
                for t in tabs {
                    let s = sources
                        .get(&t.source)
                        .ok_or_else(|| anyhow::anyhow!("Missing source"))?;
                    s.validate(t)?;
                    if !self
                        .panes
                        .iter()
                        .any(|p| p.tab.id == t.id && self.retained(p, t, s, basis))
                    {
                        Pane::new(t.clone(), s.clone())?;
                    }
                }
                for p in &self.panes {
                    let keep = tabs.iter().find(|t| t.id == p.tab.id).is_some_and(|t| {
                        self.retained(p, t, &sources[&t.source], basis)
                            && p.selected.as_ref().is_none_or(|m| {
                                sources[&t.source].members.iter().any(|n| {
                                    n.identity() == m.identity() && n.status == "available"
                                })
                            })
                    });
                    if !keep {
                        affected.push(p.id)
                    }
                }
            }
            Change::Select { pane, member } => {
                let p = self
                    .panes
                    .iter()
                    .find(|p| p.id == *pane)
                    .ok_or_else(|| anyhow::anyhow!("Stale pane"))?;
                ensure!(
                    p.source
                        .members
                        .iter()
                        .any(|m| m.identity() == member.identity() && m.status == "available"),
                    "Unavailable selection"
                );
                if p.selected
                    .as_ref()
                    .is_none_or(|m| m.identity() != member.identity())
                {
                    affected.push(p.id)
                }
            }
            Change::Remove { pane } => {
                ensure!(self.panes.iter().any(|p| p.id == *pane), "Stale pane");
                affected.push(*pane)
            }
        }
        let affected = affected
            .into_iter()
            .map(|id| {
                let p = self.panes.iter().find(|p| p.id == id).unwrap();
                (id, p.editor.as_ref().map(|e| (e.id, e.revision)))
            })
            .collect();
        Ok(Plan {
            ticket: self.intent,
            change,
            affected,
        })
    }
    pub fn needs_guard(&self, p: &Plan) -> bool {
        p.affected.iter().any(|(id, _)| {
            self.panes
                .iter()
                .find(|p| p.id == *id)
                .and_then(|p| p.editor.as_ref())
                .is_some_and(|e| e.dirty() || !e.can_leave())
        })
    }
    pub fn commit(&mut self, plan: Plan, discard: bool) -> Result<()> {
        ensure!(plan.ticket == self.intent, "Transition superseded");
        for (id, stamp) in &plan.affected {
            let p = self
                .panes
                .iter()
                .find(|p| p.id == *id)
                .ok_or_else(|| anyhow::anyhow!("Owner removed"))?;
            ensure!(
                p.editor.as_ref().map(|e| (e.id, e.revision)) == *stamp,
                "Guard superseded by new work"
            );
            if let Some(e) = &p.editor {
                ensure!(
                    e.can_leave() && (!e.dirty() || discard),
                    "Work retained: finish composition or resolve save, then choose Stay/Discard"
                );
            }
        }
        match plan.change {
            Change::Deck {
                tabs,
                sources,
                basis,
                publication,
            } => {
                // Build every replacement before changing the usable registry.
                let mut next = vec![];
                for t in tabs {
                    let s = &sources[&t.source];
                    let mut p = if let Some(p) = self
                        .panes
                        .iter()
                        .find(|p| p.tab.id == t.id && self.retained(p, &t, s, &basis))
                    {
                        p.clone()
                    } else {
                        Pane::new(t.clone(), s.clone())?
                    };
                    if let Some(m) = &p.selected {
                        if let Some(n) = s
                            .members
                            .iter()
                            .find(|n| n.identity() == m.identity() && n.status == "available")
                        {
                            p.selected = Some(n.clone())
                        } else {
                            p.selected = None;
                            p.editor = None;
                            p.selection_revision += 1;
                        }
                    }
                    p.tab = t;
                    p.source = s.clone();
                    next.push(p);
                }
                self.panes = next;
                self.basis = basis;
                self.publication = publication;
            }
            Change::Select { pane, member } => {
                let p = self.panes.iter_mut().find(|p| p.id == pane).unwrap();
                if p.selected
                    .as_ref()
                    .is_none_or(|m| m.identity() != member.identity())
                {
                    p.selected = Some(member);
                    p.editor = None;
                    p.selection_revision += 1;
                }
            }
            Change::Remove { pane } => self.panes.retain(|p| p.id != pane),
        }
        if !self.panes.iter().any(|p| Some(p.id) == self.active) {
            self.active = self.panes.first().map(|p| p.id);
        }
        for p in &mut self.panes {
            p.mount_editor(&self.publication);
            if let Some(e) = &mut p.editor {
                if Some(p.id) == self.active {
                    e.resume()
                } else {
                    e.suspend();
                }
            }
        }
        Ok(())
    }
    pub fn refuse(&mut self) {
        self.intent += 1;
    }
}
