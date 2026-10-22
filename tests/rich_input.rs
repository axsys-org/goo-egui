use eframe::egui::*;
use serde_json::{Value, json};
use shrine_egui::{
    model::*,
    rich_text::{Draft, Editing, widget},
    session::Editor,
    transport::Outcome,
};
use std::collections::BTreeMap;
fn source() -> Value {
    serde_json::from_str(include_str!("fixtures/prepared-rich-input.json")).unwrap()
}
fn contract() -> Editing {
    serde_json::from_value(source()["display"]["nodes"][0]["editing"].clone()).unwrap()
}
fn runs() -> Vec<TextRun> {
    serde_json::from_value(source()["display"]["nodes"][0]["textRuns"].clone()).unwrap()
}
fn view() -> Presentation {
    let mut p: Value = serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap();
    p["display"]["version"] = json!(3);
    let node = p["display"]["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["kind"] == "input")
        .unwrap();
    let rich = source()["display"]["nodes"][0].clone();
    for key in ["editing", "textRuns", "payload", "semantics", "textStyle"] {
        node[key] = rich[key].clone();
    }
    Presentation::decode(&serde_json::to_vec(&p).unwrap()).unwrap()
}
fn editor() -> Editor {
    let p = view();
    p.validate(&p.context, &p.snapshot.subject).unwrap();
    let mut e = Editor::new(p.snapshot.subject.clone(), "/sys/slot/content".into());
    let t = e.request(p.context.clone()).unwrap();
    e.receive(t, p).unwrap();
    e
}
fn slot(e: &Editor) -> String {
    e.rich_fields.keys().next().unwrap().clone()
}
#[test]
fn server_contract_and_typed_values_are_validated_before_mounting() {
    let p = view();
    p.validate(&p.context, &p.snapshot.subject).unwrap();
    for mutation in 0..7 {
        let mut p = view();
        let n = p
            .display
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| n.editing.is_some())
            .unwrap();
        match mutation {
            0 => n.editing.as_mut().unwrap().version = 999,
            1 => {
                n.editing.as_mut().unwrap().styles[0]
                    .resolved
                    .text_style
                    .font_sha256 = "wrong".into()
            }
            2 => {
                n.editing
                    .as_mut()
                    .unwrap()
                    .link_style
                    .highlight
                    .as_mut()
                    .unwrap()
                    .fill = "bad".into()
            }
            3 => n.editing.as_mut().unwrap().encoding = "plain".into(),
            4 => {
                n.editing
                    .as_mut()
                    .unwrap()
                    .link_style
                    .text_style
                    .font_sha256 = "wrong".into()
            }
            5 => n.editing.as_mut().unwrap().link_style.semantics.role = "button".into(),
            _ => n.editing = None,
        }
        assert!(p.validate(&p.context, &p.snapshot.subject).is_err());
    }
    let mut p = view();
    p.display.as_mut().unwrap().version = 2;
    assert!(p.validate(&p.context, &p.snapshot.subject).is_err());
    let c = contract();
    assert!(c.decode("bad").is_err());
    assert!(c.decode(&json!([{"myth":{"/sys/link":{"kind":"weblink","value":"javascript:bad"}},"lang":{"en":{"txt":"x"}}}]).to_string()).is_err());
}
#[test]
fn insert_delete_and_selection_format_preserve_other_runs() {
    let c = contract();
    let mut d = Draft::new(&runs());
    let original = d.runs();
    d.replace(6..6, "new ");
    assert_eq!(
        d.text(),
        format!(
            "Plain new {}",
            &original.iter().map(|r| r.lang.text()).collect::<String>()[6..]
        )
    );
    d.replace(6..10, "");
    assert_eq!(
        serde_json::to_value(d.runs()).unwrap(),
        serde_json::to_value(original).unwrap()
    );
    d.selection = 0..5;
    d.toggle("bold", &c).unwrap();
    let marked = d.wire();
    assert_eq!(d.runs()[0].myth["/sys/modifiers"]["value"], json!(["bold"]));
    d.undo();
    assert!(!d.runs()[0].myth.contains_key("/sys/modifiers"));
    d.redo();
    assert_eq!(d.wire(), marked);
    d.format(
        "/sys/link",
        Some(json!({"kind":"path","value":"/note"})),
        &c,
    )
    .unwrap();
    assert_eq!(d.runs()[0].myth["/sys/link"]["value"], "/note");
    let before = d.wire();
    d.toggle("italic", &c).unwrap();
    let italic = d.wire();
    assert!(
        d.format(
            "/sys/font",
            Some(json!({"kind":"path","value":"/sys/font/mono"})),
            &c
        )
        .is_err()
    );
    assert_eq!(d.wire(), italic);
    assert_ne!(before, italic);
}
#[test]
fn dirty_run_draft_survives_refresh_stale_response_and_composition() {
    let mut e = editor();
    let key = slot(&e);
    let d = e.rich_fields.get_mut(&key).unwrap();
    d.selection = 0..5;
    d.toggle("underline", &contract()).unwrap();
    let wire = d.wire();
    e.edit(&key, wire.clone());
    let p = view();
    let old = e.request(p.context.clone()).unwrap();
    let now = e.request(p.context.clone()).unwrap();
    assert!(!e.receive(old, p.clone()).unwrap());
    assert!(e.receive(now, p.clone()).unwrap());
    assert_eq!(e.fields[&key].value, wire);
    assert_eq!(e.rich_fields[&key].selection, 0..5);
    e.composing = true;
    let t = e.request(p.context.clone()).unwrap();
    assert!(!e.receive(t, p).unwrap());
    assert_eq!(e.fields[&key].value, wire);
    assert!(e.candidate.is_some());
}
#[test]
fn identity_and_encoding_replacement_are_guarded_and_supersession_retains_draft() {
    let mut e = editor();
    let key = slot(&e);
    let d = e.rich_fields.get_mut(&key).unwrap();
    d.selection = 0..5;
    d.toggle("italic", &contract()).unwrap();
    let wire = d.wire();
    e.edit(&key, wire.clone());
    let id = e.id;
    let mut p = view();
    p.snapshot.life = "replacement".into();
    let t = e.request(p.context.clone()).unwrap();
    assert!(!e.receive(t, p).unwrap());
    assert_eq!(e.id, id);
    let rev = e.revision;
    e.edit(&key, wire.clone());
    assert!(e.accept_candidate(rev).is_err());
    assert_eq!(e.fields[&key].value, wire);
    let p: Presentation = serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap();
    let t = e.request(p.context.clone()).unwrap();
    assert!(!e.receive(t, p).unwrap());
    assert_eq!(e.id, id);
    assert_eq!(e.fields[&key].value, wire);
    e.accept_candidate(e.revision).unwrap();
    assert_ne!(e.id, id);
    assert!(e.rich_fields.is_empty());
}
#[test]
fn frozen_rich_capture_exact_retry_and_late_accept_preserve_new_edits() {
    let mut e = editor();
    let key = slot(&e);
    let d = e.rich_fields.get_mut(&key).unwrap();
    d.selection = 0..5;
    d.toggle("bold", &contract()).unwrap();
    let saved = d.wire();
    e.edit(&key, saved.clone());
    let action = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .iter()
        .find(|c| c.kind == "action")
        .unwrap()
        .clone();
    let invocation = e.activate(&action, &BTreeMap::new()).unwrap();
    assert_eq!(invocation.inputs()[&key], saved);
    e.suspended = true;
    e.complete(invocation.id(), Outcome::Unknown("timeout".into()));
    let retry = e.pending.as_ref().unwrap();
    assert_eq!(retry.body(), invocation.body());
    assert_eq!(retry.id(), invocation.id());
    let mut later = Draft::new(&runs());
    later.replace(0..5, "Later");
    let newer = later.wire();
    e.edit(&key, newer.clone());
    assert!(e.complete(invocation.id(), Outcome::Accepted));
    assert_eq!(e.fields[&key].seed, saved);
    assert_eq!(e.fields[&key].value, newer);
    assert!(e.dirty());
}
#[test]
fn native_text_widget_undo_restores_text_and_metadata_together() {
    let ctx = eframe::egui::Context::default();
    shrine_egui::painter::install(&ctx);
    let id = Id::new("rich-test");
    let c = contract();
    let mut d = Draft::new(&runs());
    let initial = d.wire();
    let frame = |events: Vec<Event>, d: &mut Draft| {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 400.))),
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    widget(
                        ui,
                        id,
                        Rect::from_min_size(pos2(10., 10.), vec2(600., 250.)),
                        d,
                        &c,
                        false,
                        true,
                    );
                });
            },
        );
    };
    frame(vec![], &mut d);
    ctx.memory_mut(|m| m.request_focus(id));
    let mut state = egui_state(&ctx, id);
    state
        .cursor
        .set_char_range(Some(eframe::egui::text::CCursorRange::two(
            eframe::egui::text::CCursor::new(0),
            eframe::egui::text::CCursor::new(5),
        )));
    state.store(&ctx, id);
    frame(vec![], &mut d);
    assert_eq!(d.selection, 0..5);
    for pressed in [true, false] {
        frame(
            vec![
                Event::PointerMoved(pos2(20., 20.)),
                Event::PointerButton {
                    pos: pos2(20., 20.),
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            &mut d,
        );
        assert_eq!(d.selection, 0..5, "context menu collapsed the selection");
    }
    frame(
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        &mut d,
    );
    ctx.memory_mut(|m| m.request_focus(id));
    frame(vec![Event::Text("Replacement".into())], &mut d);
    assert!(d.text().starts_with("Replacement"));
    assert_ne!(d.wire(), initial);
    frame(
        vec![Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }],
        &mut d,
    );
    assert_eq!(d.wire(), initial);
}
fn egui_state(ctx: &eframe::egui::Context, id: Id) -> eframe::egui::text_edit::TextEditState {
    eframe::egui::text_edit::TextEditState::load(ctx, id).unwrap()
}

#[test]
fn native_ime_preedit_commits_as_one_structured_undo() {
    let ctx = eframe::egui::Context::default();
    shrine_egui::painter::install(&ctx);
    let id = Id::new("rich-ime-test");
    let c = contract();
    let mut d = Draft::new(&runs());
    d.selection = 0..5;
    d.toggle("bold", &c).unwrap();
    let initial = d.wire();
    let frame = |events: Vec<Event>, composing: bool, d: &mut Draft| {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 400.))),
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    widget(
                        ui,
                        id,
                        Rect::from_min_size(pos2(10., 10.), vec2(600., 250.)),
                        d,
                        &c,
                        composing,
                        true,
                    );
                });
            },
        );
    };
    frame(vec![], false, &mut d);
    ctx.memory_mut(|m| m.request_focus(id));
    let mut state = egui_state(&ctx, id);
    state
        .cursor
        .set_char_range(Some(eframe::egui::text::CCursorRange::two(
            eframe::egui::text::CCursor::new(0),
            eframe::egui::text::CCursor::new(5),
        )));
    state.store(&ctx, id);
    frame(
        vec![
            Event::Ime(ImeEvent::Enabled),
            Event::Ime(ImeEvent::Preedit("p".into())),
        ],
        true,
        &mut d,
    );
    frame(
        vec![Event::Ime(ImeEvent::Preedit("pre".into()))],
        true,
        &mut d,
    );
    frame(
        vec![Event::Ime(ImeEvent::Commit("Composed".into()))],
        false,
        &mut d,
    );
    assert!(d.text().starts_with("Composed"), "{}", d.text());
    assert_eq!(d.runs()[0].myth["/sys/modifiers"]["value"], json!(["bold"]));
    let committed = d.wire();
    d.undo();
    assert_eq!(d.wire(), initial);
    d.redo();
    assert_eq!(d.wire(), committed);
}

#[test]
#[ignore = "Mutates only an explicitly opted-in disposable rich-input fixture"]
fn live_rich_save_unknown_exact_retry_and_reread() {
    use shrine_egui::transport::Client;
    use std::time::Duration;
    assert_eq!(std::env::var("GOO_DISPOSABLE").unwrap(), "1");
    let client = Client::new(&std::env::var("GOO_TEST_SERVER").unwrap()).unwrap();
    let delayed = Client::with_timeout(
        &std::env::var("GOO_TEST_PROXY").unwrap(),
        Duration::from_millis(350),
    )
    .unwrap();
    let target = "/0x11/app/ios_demo/note";
    let ctx = shrine_egui::model::Context::new(480.);
    let p = client
        .render(target, "slot", "/sys/slot/content", &ctx)
        .unwrap();
    p.validate(&ctx, target).unwrap();
    std::fs::write(
        "evidence/rich-input-live-view.json",
        serde_json::to_vec(&p.received_value().unwrap()).unwrap(),
    )
    .unwrap();
    let mut e = Editor::new(target.into(), "/sys/slot/content".into());
    let t = e.request(ctx.clone()).unwrap();
    e.receive(t, p).unwrap();
    let key = slot(&e);
    let original = e.fields[&key].value.clone();
    let contract = e
        .presentation
        .as_ref()
        .unwrap()
        .display
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|n| n.editing.clone())
        .unwrap();
    let draft = e.rich_fields.get_mut(&key).unwrap();
    draft.selection = 0..5;
    draft.toggle("underline", &contract).unwrap();
    draft.replace(0..0, "Edited\n");
    let saved = draft.wire();
    e.edit(&key, saved.clone());
    let narrow = shrine_egui::model::Context::new(300.);
    let t = e.request(narrow.clone()).unwrap();
    e.receive(t, client.render(target, "slot", &e.slot, &narrow).unwrap())
        .unwrap();
    assert_eq!(e.fields[&key].value, saved);
    let c = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .iter()
        .find(|c| c.kind == "action")
        .unwrap()
        .clone();
    let invocation = e.activate(&c, &BTreeMap::new()).unwrap();
    let outcome = delayed.activate(&invocation);
    assert!(matches!(outcome, Outcome::Unknown(_)), "{outcome:?}");
    e.suspended = true;
    e.complete(invocation.id(), outcome);
    assert!(e.pending.is_some());
    assert_eq!(client.status(&invocation), Outcome::Accepted);
    let reread = client.render(target, "slot", &e.slot, &ctx).unwrap();
    reread.validate(&ctx, target).unwrap();
    assert_eq!(reread.seeds()[&key], saved);
    let retry = e.pending.as_ref().unwrap();
    assert_eq!(retry.body(), invocation.body());
    assert_eq!(retry.id(), invocation.id());
    assert_eq!(client.activate(retry), Outcome::Accepted);
    let again = client.render(target, "slot", &e.slot, &ctx).unwrap();
    assert_eq!(
        again.snapshot.expected, reread.snapshot.expected,
        "exact retry executed twice"
    );
    e.complete(invocation.id(), Outcome::Accepted);
    assert!(e.suspended);
    e.suspended = false;
    let t = e.request(ctx.clone()).unwrap();
    e.receive(t, again).unwrap();
    assert!(!e.dirty());
    // Invalid typed value must reject without changing persisted text.
    let mut bad = e.values();
    bad.insert(key.clone(), "not JSON".into());
    assert!(
        shrine_egui::transport::Invocation::capture(
            e.presentation.as_ref().unwrap(),
            &c,
            bad,
            &BTreeMap::new()
        )
        .is_err()
    );
    e.edit(&key, original.clone());
    let c = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .iter()
        .find(|c| c.kind == "action")
        .unwrap()
        .clone();
    let restore = e.activate(&c, &BTreeMap::new()).unwrap();
    assert_eq!(client.activate(&restore), Outcome::Accepted);
    let restored = client.render(target, "slot", &e.slot, &ctx).unwrap();
    assert_eq!(restored.seeds()[&key], original);
    println!(
        "PASS rich typed save and persisted reread, dirty resize, delayed unknown while suspended, receipt, exact retry without second mutation, restoration"
    );
}

#[test]
fn rich_secondary_click_does_not_open_inspector_or_invoke_save() {
    let ctx = eframe::egui::Context::default();
    shrine_egui::painter::install(&ctx);
    let mut e = editor();
    let before = e.values();
    let node = e
        .presentation
        .as_ref()
        .unwrap()
        .display
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.editing.is_some())
        .unwrap();
    let point = pos2(18. + node.r#box.x as f32, 18. + node.r#box.y as f32);
    for pressed in [true, false] {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 900.))),
                events: vec![
                    Event::PointerMoved(point),
                    Event::PointerButton {
                        pos: point,
                        button: PointerButton::Secondary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    let (action, inspect) =
                        shrine_egui::painter::surface(ui, &mut e, &BTreeMap::new());
                    assert!(action.is_none());
                    assert!(inspect.is_none());
                });
            },
        );
    }
    assert_eq!(e.values(), before);
}

#[test]
fn independent_rich_owners_survive_tab_switch_reorder_and_refused_removal() {
    use shrine_egui::session::{Change, Session};
    let deck: Presentation = serde_json::from_str(include_str!("fixtures/deck.json")).unwrap();
    let source: Source = serde_json::from_str(include_str!("fixtures/deck-source.json")).unwrap();
    let mut s = Session::default();
    let plan = s
        .stage(Change::Deck {
            tabs: tabs(&deck).unwrap(),
            sources: BTreeMap::from([(source.id.clone(), source)]),
            basis: deck.lifetime(),
            publication: deck.snapshot.publication,
        })
        .unwrap();
    s.commit(plan, false).unwrap();
    let mut owners = vec![];
    for index in 0..2 {
        let p = &s.panes[index];
        let id = p.id;
        let plan = s
            .stage(Change::Select {
                pane: id,
                member: p.source.members[index].clone(),
            })
            .unwrap();
        s.commit(plan, false).unwrap();
        s.switch(id);
        let e = s.panes[index].editor.as_mut().unwrap();
        let mut p = view();
        p.snapshot.subject = e.target.clone();
        p.snapshot.target = e.target.clone();
        let t = e.request(p.context.clone()).unwrap();
        e.receive(t, p).unwrap();
        let key = slot(e);
        let d = e.rich_fields.get_mut(&key).unwrap();
        d.selection = 0..5;
        d.toggle(if index == 0 { "bold" } else { "italic" }, &contract())
            .unwrap();
        let wire = d.wire();
        e.edit(&key, wire.clone());
        owners.push((id, e.id, key, wire));
    }
    s.switch(owners[0].0);
    s.panes.reverse();
    for (_, id, key, wire) in &owners {
        let e = s.editor_mut(*id).unwrap();
        assert_eq!(&e.fields[key].value, wire);
        assert_eq!(&e.rich_fields[key].wire(), wire);
    }
    let plan = s.stage(Change::Remove { pane: owners[0].0 }).unwrap();
    assert!(s.commit(plan.clone(), false).is_err());
    s.refuse();
    assert!(s.commit(plan, true).is_err());
    assert_eq!(s.panes.len(), 2);
}
