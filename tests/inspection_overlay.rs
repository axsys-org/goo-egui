use eframe::egui::*;
use serde_json::{Value, json};
use shrine_egui::{
    inspector::overlay::{Overlay, annotations, hit_node},
    model::Presentation,
    painter,
};

fn fixture() -> (Presentation, Value) {
    let raw: Value = serde_json::from_str(include_str!("fixtures/live-pipeline.json")).unwrap();
    let p = Presentation::decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
    p.validate(&p.context, &p.snapshot.subject).unwrap();
    (p, raw)
}
fn stage(raw: &Value, name: &str) -> Value {
    raw["pipeline"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap()["output"]
        .clone()
}
#[test]
fn annotations_join_only_explicit_ids_and_use_received_geometry() {
    let (p, raw) = fixture();
    let d = p.display.as_ref().unwrap();
    let marks = annotations(d, "prepared", &stage(&raw, "prepared"), None);
    let baselines: Vec<_> = marks.iter().filter_map(|m| m.line).collect();
    let expected: Vec<_> = d
        .nodes
        .iter()
        .flat_map(|n| n.lines.iter())
        .map(|l| {
            [
                pos2(l.r#box.x as f32, l.baseline as f32),
                pos2((l.r#box.x + l.r#box.width) as f32, l.baseline as f32),
            ]
        })
        .collect();
    assert_eq!(baselines, expected);
    assert!(annotations(d, "source", &stage(&raw, "source"), None).is_empty());
    assert!(
        annotations(
            d,
            "bindings",
            &json!([{"binding":{"nodeId":"absent","presented":true}}]),
            None
        )
        .is_empty()
    );
    assert!(
        annotations(
            d,
            "bindings",
            &json!([{"binding":{"nodeId":d.nodes[0].id,"presented":false}}]),
            None
        )
        .is_empty()
    );
    let binding = annotations(d, "bindings", &stage(&raw, "bindings"), None);
    assert_eq!(binding.len(), 3);
    let n = d.nodes.iter().find(|n| n.kind == "input").unwrap();
    let picked = hit_node(d, pos2(n.r#box.x as f32 + 5., n.r#box.y as f32 + 5.));
    assert_eq!(picked.as_deref(), Some(n.id.as_str()));
    let layout = annotations(d, "layout", &stage(&raw, "layout"), None);
    assert_eq!(layout.len(), 1);
    assert_eq!(layout[0].rect.unwrap().width(), 480.);
    let trace = annotations(d, "trace", &stage(&raw, "trace"), Some(&n.id));
    assert!(!trace.is_empty());
    assert!(trace.iter().all(|m| m.line.is_some()));
}
#[test]
fn frozen_fragments_validate_independently_and_reject_bad_profiles_or_geometry() {
    let (p, raw) = fixture();
    let nested = json!({"content":[raw.clone(),raw.clone()]});
    let overlay = Overlay::capture(&p, &nested, None);
    assert_eq!(overlay.fragments.len(), 2);
    assert_eq!(overlay.fragments[0].path, "/content/0");
    assert_eq!(overlay.fragments[1].path, "/content/1");
    let mut bad = raw.clone();
    bad["display"]["nodes"][0]["box"]["width"] = json!(-1);
    let overlay = Overlay::capture(&p, &json!({"content":[raw.clone(),bad]}), None);
    assert_eq!(overlay.fragments.len(), 1);
    assert_eq!(overlay.errors.len(), 1);
    assert!(overlay.errors[0].contains("/content/1"));
    let mut bad = raw.clone();
    bad["profile"]["version"] = json!(999);
    assert!(Overlay::capture(&p, &bad, None).fragments.is_empty());
    let mut newer = raw.clone();
    newer["display"]["nodes"][0]["payload"] = json!("replacement");
    assert_ne!(
        overlay.fragments[0]
            .presentation
            .display
            .as_ref()
            .unwrap()
            .nodes[0]
            .payload,
        "replacement"
    );
    let mut unsupported = raw;
    unsupported["pipeline"]["version"] = json!(999);
    let overlay = Overlay::capture(&p, &unsupported, None);
    assert!(overlay.fragments[0].stage("display").is_err());
}
#[test]
fn picking_respects_ancestor_clipping_and_paint_order() {
    let (p, _) = fixture();
    let mut d = p.display.unwrap();
    d.nodes.truncate(2);
    d.nodes[0].r#box.width = 100.;
    d.nodes[0].r#box.height = 100.;
    d.nodes[0].paint.clip = true;
    d.nodes[0].paint.corners.kind = "ellipse".into();
    d.nodes[1].r#box = d.nodes[0].r#box.clone();
    assert!(hit_node(&d, pos2(1., 1.)).is_none());
    assert_eq!(
        hit_node(&d, pos2(50., 50.)).as_deref(),
        Some(d.nodes[1].id.as_str())
    );
    assert!(hit_node(&d, pos2(120., 50.)).is_none());
}
#[test]
fn snapshot_painter_has_no_edit_or_invocation_path() {
    let (p, _) = fixture();
    let before = serde_json::to_value(&p).unwrap();
    let ctx = Context::default();
    painter::install(&ctx);
    let n = p
        .display
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.kind == "input")
        .unwrap();
    let pos = pos2(n.r#box.x as f32 + 20., n.r#box.y as f32 + 20.);
    for events in [
        vec![],
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        vec![
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("must not edit".into()),
        ],
    ] {
        let output = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 700.))),
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    let r = painter::snapshot(ui, &p);
                    assert_eq!(r.width(), 480.);
                });
            },
        );
        assert!(!output.shapes.is_empty());
        assert!(ctx.memory(|m| m.focused()).is_none());
    }
    assert_eq!(serde_json::to_value(&p).unwrap(), before);
}
