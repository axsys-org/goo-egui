use shrine_egui::{model::*, session::*, transport::*};
use std::collections::BTreeMap;
fn view() -> Presentation {
    serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap()
}
fn editor() -> Editor {
    let p = view();
    let mut e = Editor::new(p.snapshot.subject.clone(), "/sys/slot/content".into());
    let t = e.request(p.context.clone()).unwrap();
    assert!(e.receive(t, p).unwrap());
    e
}
fn deck() -> Change {
    let p: Presentation = serde_json::from_str(include_str!("fixtures/deck.json")).unwrap();
    let source: Source = serde_json::from_str(include_str!("fixtures/deck-source.json")).unwrap();
    Change::Deck {
        tabs: tabs(&p).unwrap(),
        sources: BTreeMap::from([(source.id.clone(), source)]),
        basis: p.lifetime(),
        publication: p.snapshot.publication,
    }
}
fn session() -> Session {
    let mut s = Session::default();
    let p = s.stage(deck()).unwrap();
    s.commit(p, false).unwrap();
    s
}
fn select(s: &mut Session, index: usize) {
    let p = &s.panes[index];
    let change = Change::Select {
        pane: p.id,
        member: p.source.members[index].clone(),
    };
    let plan = s.stage(change).unwrap();
    s.commit(plan, false).unwrap();
    s.switch(s.panes[index].id);
    let mut fixture = view();
    let e = s.panes[index].editor.as_mut().unwrap();
    fixture.snapshot.subject = e.target.clone();
    fixture.snapshot.target = e.target.clone();
    let t = e.request(fixture.context.clone()).unwrap();
    e.receive(t, fixture).unwrap();
}
#[test]
fn admits_shared_live_view() {
    let p = view();
    p.validate(&p.context, &p.snapshot.subject).unwrap();
}
#[test]
fn rejects_profile_context_and_font_mismatch() {
    let p = view();
    let mut q = p.clone();
    q.profile.revision = "wrong".into();
    assert!(q.validate(&p.context, &p.snapshot.subject).is_err());
    assert!(
        p.validate(&Context::new(500.), &p.snapshot.subject)
            .is_err()
    );
    let mut q = p.clone();
    q.display.as_mut().unwrap().nodes[0].text_style.font_sha256 = "wrong".into();
    assert!(q.validate(&p.context, &p.snapshot.subject).is_err());
}
#[test]
fn rejects_bad_geometry_duplicate_and_unbound_nodes() {
    for mode in 0..3 {
        let mut p = view();
        let d = p.display.as_mut().unwrap();
        match mode {
            0 => d.nodes[0].r#box.width = f64::NAN,
            1 => d.nodes[1].id = d.nodes[0].id.clone(),
            _ => {
                p.controls.remove(0);
            }
        };
        assert!(p.validate(&p.context, &p.snapshot.subject).is_err());
    }
}
#[test]
fn hidden_control_does_not_create_input() {
    let mut p = view();
    let c = &mut p.controls[0];
    c.binding.presented = false;
    assert!(p.validate(&p.context, &p.snapshot.subject).is_err());
    let mut p = view();
    let c = p.controls[0].clone();
    p.display.as_mut().unwrap().nodes.retain(|n| n.id != c.id);
    p.controls[0].binding.presented = false;
    p.validate(&p.context, &p.snapshot.subject).unwrap();
    assert!(!p.seeds().contains_key(&c.slot));
}
#[test]
fn descriptive_button_does_not_authorize_action() {
    let mut p = view();
    p.controls.retain(|c| c.kind == "input");
    p.validate(&p.context, &p.snapshot.subject).unwrap();
    let c = view().controls.last().unwrap().clone();
    assert!(Invocation::capture(&p, &c, p.seeds(), &BTreeMap::new()).is_err());
}
#[test]
fn stale_render_response_is_ignored() {
    let mut e = editor();
    let t = e.request(view().context).unwrap();
    let latest = e.request(view().context).unwrap();
    assert!(!e.receive(t, view()).unwrap());
    assert!(e.receive(latest, view()).unwrap());
}
#[test]
fn draft_survives_refresh_and_resize() {
    let mut e = editor();
    let id = e.id;
    e.edit("/sys/req", "draft".into());
    let mut p = view();
    p.context.width = 500.;
    p.display.as_mut().unwrap().context = p.context.clone();
    p.display.as_mut().unwrap().width = 500.;
    let t = e.request(p.context.clone()).unwrap();
    e.receive(t, p).unwrap();
    assert_eq!(e.fields["/sys/req"].value, "draft");
    assert_eq!(e.id, id);
    assert!(e.dirty());
}
#[test]
fn identity_replacement_stages_dirty_work() {
    let mut e = editor();
    e.edit("/sys/req", "draft".into());
    let id = e.id;
    let mut p = view();
    p.snapshot.life = "2".into();
    let t = e.request(p.context.clone()).unwrap();
    assert!(!e.receive(t, p).unwrap());
    assert_eq!(e.id, id);
    assert_eq!(e.fields["/sys/req"].value, "draft");
    let rev = e.revision;
    e.edit("/sys/req", "new work".into());
    assert!(e.accept_candidate(rev).is_err());
    assert_eq!(e.id, id);
}
#[test]
fn composition_defers_preparation_and_suspension() {
    let mut e = editor();
    let id = e.id;
    e.composing = true;
    let t = e.request(view().context).unwrap();
    assert!(!e.receive(t, view()).unwrap());
    assert!(!e.suspend());
    assert_eq!(e.id, id);
    assert!(e.candidate.is_some());
}
#[test]
fn frozen_captures_and_exact_retry() {
    let mut e = editor();
    let mut p = e.presentation.clone().unwrap();
    let c = p.controls.last_mut().unwrap();
    c.captures = vec![CaptureRef {
        slot: "/capture".into(),
        selection: "/selection".into(),
    }];
    c.binding.captures = c.captures.clone();
    let c = c.clone();
    e.presentation = Some(p);
    let source: Source = serde_json::from_str(include_str!("fixtures/deck-source.json")).unwrap();
    let mut captures = BTreeMap::from([(
        "/selection".into(),
        SelectionCapture {
            selection: "owner".into(),
            incarnation: 1,
            revision: 3,
            member: source.members[0].clone(),
        },
    )]);
    e.edit("/sys/req", "frozen".into());
    let i = e.activate(&c, &captures).unwrap();
    captures.get_mut("/selection").unwrap().member = source.members[1].clone();
    e.edit("/sys/req", "later".into());
    e.complete(i.id(), Outcome::Unknown("lost".into()));
    let retry = e.pending.as_ref().unwrap();
    assert_eq!(i.body(), retry.body());
    assert_eq!(i.id(), retry.id());
    let f: BTreeMap<_, _> = url::form_urlencoded::parse(i.body().as_bytes())
        .into_owned()
        .collect();
    assert_eq!(f["input:/sys/req"], "frozen");
    assert!(f["input:/capture"].contains("notes/a"));
    assert_eq!(f["subject"], view().snapshot.target);
    assert_eq!(f["publication"], view().snapshot.publication);
}
#[test]
fn accepted_save_keeps_later_typing_dirty() {
    let mut e = editor();
    e.edit("/sys/req", "submitted".into());
    let c = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .last()
        .unwrap()
        .clone();
    let i = e.activate(&c, &BTreeMap::new()).unwrap();
    e.edit("/sys/req", "later".into());
    assert!(e.complete(i.id(), Outcome::Accepted));
    assert_eq!(e.fields["/sys/req"].seed, "submitted");
    assert_eq!(e.fields["/sys/req"].value, "later");
    assert!(e.dirty());
    assert!(!e.can_act());
}
#[test]
fn rejected_differs_from_unknown() {
    for outcome in [
        Outcome::Rejected("no".into()),
        Outcome::Unknown("lost".into()),
    ] {
        let mut e = editor();
        let c = e
            .presentation
            .as_ref()
            .unwrap()
            .controls
            .last()
            .unwrap()
            .clone();
        let i = e.activate(&c, &BTreeMap::new()).unwrap();
        e.complete(i.id(), outcome.clone());
        assert_eq!(e.pending.is_some(), matches!(outcome, Outcome::Unknown(_)));
        assert!(!e.can_act());
    }
}
#[test]
fn dirty_removal_refusal_and_supersession_preserve_owner() {
    let mut s = session();
    select(&mut s, 0);
    let id = s.panes[0].id;
    s.panes[0]
        .editor
        .as_mut()
        .unwrap()
        .edit("/sys/req", "draft".into());
    let p = s.stage(Change::Remove { pane: id }).unwrap();
    assert!(s.needs_guard(&p));
    assert!(s.commit(p.clone(), false).is_err());
    assert_eq!(s.panes.len(), 2);
    s.refuse();
    assert!(s.commit(p, true).is_err());
    let p = s.stage(Change::Remove { pane: id }).unwrap();
    s.panes[0]
        .editor
        .as_mut()
        .unwrap()
        .edit("/sys/req", "new".into());
    assert!(s.commit(p, true).is_err());
    assert_eq!(s.panes[0].id, id);
}
#[test]
fn reorder_preserves_independent_owners_and_drafts() {
    let mut s = session();
    select(&mut s, 0);
    select(&mut s, 1);
    for (i, p) in s.panes.iter_mut().enumerate() {
        p.editor
            .as_mut()
            .unwrap()
            .edit("/sys/req", format!("draft {i}"));
    }
    let id = s.panes[0].id;
    let mut change = deck();
    if let Change::Deck { tabs, .. } = &mut change {
        tabs.reverse();
    }
    let p = s.stage(change).unwrap();
    assert!(!s.needs_guard(&p));
    s.commit(p, false).unwrap();
    assert_eq!(s.panes[1].id, id);
    assert_eq!(
        s.panes[1].editor.as_ref().unwrap().fields["/sys/req"].value,
        "draft 0"
    );
    assert_eq!(
        s.panes[0].editor.as_ref().unwrap().fields["/sys/req"].value,
        "draft 1"
    );
}
#[test]
fn recreation_does_not_inherit_old_authority() {
    let mut s = session();
    let id = s.panes[0].id;
    let p = s.stage(Change::Remove { pane: id }).unwrap();
    s.commit(p, true).unwrap();
    let p = s.stage(deck()).unwrap();
    s.commit(p, false).unwrap();
    assert_ne!(s.panes[0].id, id);
    assert!(!s.switch(id));
}
#[test]
fn hidden_save_completion_updates_origin_only() {
    let mut s = session();
    select(&mut s, 0);
    select(&mut s, 1);
    let a = s.panes[0].id;
    let b = s.panes[1].id;
    s.switch(a);
    let e = s.panes[0].editor.as_mut().unwrap();
    e.stale = false;
    e.edit("/sys/req", "saved A".into());
    let c = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .last()
        .unwrap()
        .clone();
    let i = e.activate(&c, &BTreeMap::new()).unwrap();
    let owner = e.id;
    s.panes[1]
        .editor
        .as_mut()
        .unwrap()
        .edit("/sys/req", "dirty B".into());
    assert!(s.switch(b));
    assert!(
        s.editor_mut(owner)
            .unwrap()
            .complete(i.id(), Outcome::Accepted)
    );
    assert_eq!(
        s.panes[1].editor.as_ref().unwrap().fields["/sys/req"].value,
        "dirty B"
    );
    assert!(s.panes[0].editor.as_ref().unwrap().suspended);
    s.switch(a);
    assert!(!s.panes[0].editor.as_ref().unwrap().can_act());
}
#[test]
fn unknown_save_blocks_removal() {
    let mut s = session();
    select(&mut s, 0);
    let p = &mut s.panes[0];
    let e = p.editor.as_mut().unwrap();
    let c = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .last()
        .unwrap()
        .clone();
    let i = e.activate(&c, &BTreeMap::new()).unwrap();
    e.complete(i.id(), Outcome::Unknown("lost".into()));
    let id = p.id;
    let p = s.stage(Change::Remove { pane: id }).unwrap();
    assert!(s.commit(p, true).is_err());
    assert!(s.panes[0].editor.as_ref().unwrap().pending.is_some());
}
#[test]
fn invalid_structural_candidate_cannot_retire_anything() {
    let mut s = session();
    select(&mut s, 0);
    let id = s.panes[0].id;
    let mut d = deck();
    if let Change::Deck { sources, .. } = &mut d {
        sources.values_mut().next().unwrap().members[0].source_life = "bad".into();
    }
    assert!(s.stage(d).is_err());
    assert_eq!(s.panes[0].id, id);
    assert!(s.panes[0].editor.is_some());
}
#[test]
fn source_replacement_is_guarded_atomically() {
    let mut s = session();
    select(&mut s, 0);
    let id = s.panes[0].id;
    s.panes[0]
        .editor
        .as_mut()
        .unwrap()
        .edit("/sys/req", "keep".into());
    let mut d = deck();
    if let Change::Deck { sources, .. } = &mut d {
        for source in sources.values_mut() {
            source.life = "new".into();
            for m in &mut source.members {
                m.source_life = "new".into();
            }
        }
    }
    let p = s.stage(d).unwrap();
    assert!(s.commit(p.clone(), false).is_err());
    assert_eq!(s.panes[0].id, id);
    s.commit(p, true).unwrap();
    assert_ne!(s.panes[0].id, id);
    assert!(s.panes[0].selected.is_none());
}
#[test]
fn shared_corpus_has_explicit_admission_or_diagnostic() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/portable-displays.json")).unwrap();
    let cases = corpus.as_array().unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        let mut p = view();
        p.display = Some(serde_json::from_value(case["display"].clone()).unwrap());
        p.context = p.display.as_ref().unwrap().context.clone();
        p.controls.clear();
        let result = p.validate(&p.context, &p.snapshot.subject);
        let name = case["name"].as_str().unwrap();
        if !["decorated", "scaled-controls", "editable-whitespace"].contains(&name) {
            assert!(result.is_ok(), "{name}: {result:?}");
        } else {
            assert!(
                result.is_err(),
                "{name}: unexpectedly admitted outside advertised slice"
            );
        }
    }
}
#[test]
fn painter_mounts_prepared_inputs_without_relayout_or_panic() {
    let ctx = eframe::egui::Context::default();
    shrine_egui::painter::install(&ctx);
    shrine_egui::chrome::install(&ctx);
    let mut e = editor();
    for _ in 0..3 {
        let _ = ctx.run(eframe::egui::RawInput::default(), |ctx| {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                shrine_egui::painter::surface(ui, &mut e, &BTreeMap::new());
            });
        });
    }
    // Real profile insets are total horizontal/vertical padding, not per-edge values.
    let n = e
        .presentation
        .as_ref()
        .unwrap()
        .display
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.kind == "input")
        .unwrap();
    assert!(n.r#box.height > n.text_style.inset_y);
    assert!(n.r#box.height < 2. * n.text_style.inset_y);
}
#[test]
fn pinned_font_advances_match_prepared_lines() {
    let p = view();
    for n in &p.display.as_ref().unwrap().nodes {
        let face = ttf_parser::Face::parse(
            FONT_FILES
                .iter()
                .find(|(id, _)| *id == n.text_style.font_id)
                .unwrap()
                .1,
            0,
        )
        .unwrap();
        for l in &n.lines {
            let advance = l
                .text
                .chars()
                .map(|c| {
                    face.glyph_index(c)
                        .and_then(|g| face.glyph_hor_advance(g))
                        .unwrap() as f64
                        * n.text_style.font_size
                        / face.units_per_em() as f64
                })
                .sum::<f64>();
            let advance = advance + n.text_style.letter_spacing * l.text.chars().count() as f64;
            let expected = l.r#box.width - l.prefix_advance;
            assert!(
                (advance - expected).abs() < 0.25,
                "{}: {advance} vs {expected}",
                l.text
            );
        }
    }
}
#[test]
fn native_editor_accepts_pointer_keyboard_and_preserves_cursor_on_refresh() {
    use eframe::egui::{self, *};
    let ctx = egui::Context::default();
    shrine_egui::painter::install(&ctx);
    shrine_egui::chrome::install(&ctx);
    let mut e = editor();
    let frame = |e: &mut Editor, events| {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 700.))),
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    shrine_egui::painter::surface(ui, e, &BTreeMap::new());
                });
            },
        );
    };
    frame(&mut e, vec![]);
    frame(&mut e, vec![]);
    let n = e
        .presentation
        .as_ref()
        .unwrap()
        .display
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.kind == "input")
        .unwrap();
    let pos = pos2(8. + n.r#box.x as f32 + 30., 8. + n.r#box.y as f32 + 19.);
    let id = Id::new((e.id, "/sys/req"));
    frame(
        &mut e,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ],
    );
    frame(
        &mut e,
        vec![egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        }],
    );
    assert_eq!(ctx.memory(|m| m.focused()), Some(id));
    frame(&mut e, vec![egui::Event::Text("Z".into())]);
    assert!(e.fields["/sys/req"].value.contains('Z'));
    let before = TextEdit::load_state(&ctx, id).unwrap().cursor.char_range();
    let t = e.request(view().context).unwrap();
    e.receive(t, view()).unwrap();
    frame(&mut e, vec![]);
    assert_eq!(
        TextEdit::load_state(&ctx, id).unwrap().cursor.char_range(),
        before
    );
}
#[test]
fn egui_raster_font_uses_css_em_size() {
    let ctx = eframe::egui::Context::default();
    shrine_egui::painter::install(&ctx);
    shrine_egui::chrome::install(&ctx);
    let p = view();
    let _ = ctx.run(Default::default(), |ctx| {
        ctx.fonts_mut(|f| {
            for n in &p.display.as_ref().unwrap().nodes {
                let face = ttf_parser::Face::parse(
                    FONT_FILES
                        .iter()
                        .find(|(id, _)| *id == n.text_style.font_id)
                        .unwrap()
                        .1,
                    0,
                )
                .unwrap();
                let g = f.layout_no_wrap(
                    "M".into(),
                    shrine_egui::painter::font(&n.text_style),
                    eframe::egui::Color32::BLACK,
                );
                let expected = face
                    .glyph_hor_advance(face.glyph_index('M').unwrap())
                    .unwrap() as f64
                    * n.text_style.font_size
                    / face.units_per_em() as f64;
                assert!(
                    (g.rows[0].glyphs[0].advance_width as f64 - expected).abs() < 0.05,
                    "Raster font size disagrees with prepared geometry"
                );
            }
        });
    });
}
