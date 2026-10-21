use eframe::egui::{self, *};
use serde_json::{Value, json};
use shrine_egui::{inspector::Inspection, model::Presentation, painter, session::Editor};
use std::collections::BTreeMap;

fn received() -> (Editor, String) {
    let mut raw: Value = serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap();
    raw["futureEnvelopeField"] = json!({"preserve": true});
    raw["display"]["nodes"][0]["futureNodeField"] = json!([1, 2, 3]);
    let body = format!("\n{}\n", serde_json::to_string_pretty(&raw).unwrap());
    let p = Presentation::decode(body.as_bytes()).unwrap();
    let mut e = Editor::new(p.snapshot.subject.clone(), "/sys/slot/content".into());
    let ticket = e.request(p.context.clone()).unwrap();
    e.receive(ticket, p).unwrap();
    (e, body)
}

#[test]
fn inspection_retains_exact_body_unknown_fields_and_control_joins() {
    let (e, body) = received();
    let i = Inspection::capture(&e, None).unwrap();
    assert_eq!(&*i.source, body);
    assert_eq!(i.received["futureEnvelopeField"]["preserve"], true);
    assert_eq!(
        i.component(i.selected.as_deref().unwrap()).unwrap()["node"]["futureNodeField"],
        json!([1, 2, 3])
    );
    let c = &e.presentation.as_ref().unwrap().controls[0];
    let component = i.component(&c.binding.node_id).unwrap();
    assert_eq!(
        component["controls"][0]["binding"]["route"],
        c.binding.route
    );
    assert!(i.component("nonexistent").is_none());
}

#[test]
fn inspection_is_frozen_to_mounted_generation_and_never_contains_local_drafts() {
    let (mut e, body) = received();
    let p = e.presentation.as_ref().unwrap().clone();
    let slot = e.fields.keys().next().unwrap().clone();
    e.edit(&slot, "LOCAL ONLY DRAFT".into());
    let ticket = e.request(p.context.clone()).unwrap();
    let i = Inspection::capture(&e, None).unwrap();
    assert_eq!(i.generation, ticket - 1);
    assert!(!i.source.contains("LOCAL ONLY DRAFT"));
    let mut raw: Value = serde_json::from_str(&body).unwrap();
    raw["futureEnvelopeField"] = json!("new response");
    let newer = Presentation::decode(serde_json::to_string(&raw).unwrap().as_bytes()).unwrap();
    e.receive(ticket, newer).unwrap();
    e.id = uuid::Uuid::new_v4();
    assert_ne!(i.owner, e.id);
    assert_eq!(&*i.source, body);
    assert_eq!(e.fields[&slot].value, "LOCAL ONLY DRAFT");
}

#[test]
fn right_click_inspects_button_and_input_without_invoking_or_editing() {
    let (mut e, _) = received();
    let ctx = egui::Context::default();
    painter::install(&ctx);
    let frame = |e: &mut Editor, events| {
        let mut result = (None, None);
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 700.))),
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    result = painter::surface(ui, e, &BTreeMap::new());
                });
            },
        );
        result
    };
    frame(&mut e, vec![]);
    frame(&mut e, vec![]);
    let nodes = e
        .presentation
        .as_ref()
        .unwrap()
        .display
        .as_ref()
        .unwrap()
        .nodes
        .clone();
    let before = e.values();
    for kind in ["button", "input"] {
        let n = nodes.iter().find(|n| n.kind == kind).unwrap();
        let pos = pos2(8. + n.r#box.x as f32 + 15., 8. + n.r#box.y as f32 + 10.);
        frame(
            &mut e,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Secondary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        let (action, inspected) = frame(
            &mut e,
            vec![Event::PointerButton {
                pos,
                button: PointerButton::Secondary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        );
        assert!(action.is_none());
        assert_eq!(inspected.as_deref(), Some(n.id.as_str()));
    }
    assert_eq!(e.values(), before);
    assert!(e.pending.is_none());
}

fn traced_editor() -> Editor {
    let (e, _) = received();
    let mut raw: Value = serde_json::from_str(
        e.presentation
            .as_ref()
            .unwrap()
            .received_json
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    let id = raw["display"]["nodes"][0]["id"].clone();
    raw["pipeline"] = json!({"version":1,"origin":"live-preparation","stages":[
        {"name":"parse","status":"skipped","output":null,"diagnostics":[]},
        {"name":"source","status":"ok","output":{"constructor":"text","text":"server value"},"diagnostics":[]},
        {"name":"normalized","status":"ok","output":{"attrs":{"id":"parent"},"children":[{"attrs":{"id":id},"text":"server value","future":true},{"attrs":{"id":"other"},"text":"other"}]},"diagnostics":[]},
        {"name":"metrics","status":"ok","output":{"width":480,"sizes":{id.as_str().unwrap():{"min":10,"pref":20},"other":{"min":30}}},"diagnostics":[]},
        {"name":"trace","status":"ok","output":[{"leftId":id,"rightId":"other","gap":3},{"leftId":"unrelated","rightId":"also-unrelated"}],"diagnostics":[]},
        {"name":"bindings","status":"ok","output":raw["controls"],"diagnostics":[]},
        {"name":"display","status":"ok","output":raw["display"],"diagnostics":[]},
        {"name":"html","status":"ok","output":"<div>server value</div>","diagnostics":[]}
    ]});
    let p = Presentation::decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let mut e = Editor::new(p.snapshot.subject.clone(), "/sys/slot/content".into());
    let ticket = e.request(p.context.clone()).unwrap();
    e.receive(ticket, p).unwrap();
    e
}

#[test]
fn pipeline_filters_explicit_ids_and_keeps_pre_identity_stages_whole() {
    let e = traced_editor();
    let mut i = Inspection::capture(&e, None).unwrap();
    assert_eq!(
        i.stage_output("source", true).unwrap(),
        i.stage_output("source", false).unwrap()
    );
    assert!(i.stage_output("parse", true).unwrap().is_null());
    let matched = i.stage_output("normalized", true).unwrap();
    assert_eq!(matched.as_array().unwrap().len(), 1);
    assert_eq!(matched[0]["future"], true);
    assert_eq!(i.stage_output("metrics", true).unwrap()["size"]["min"], 10);
    assert_eq!(
        i.stage_output("trace", true)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        i.stage_output("display", true).unwrap()[0],
        i.component(i.selected.as_deref().unwrap()).unwrap()["node"]
    );
    let input = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .iter()
        .find(|c| c.kind == "input")
        .unwrap();
    i.select(Some(input.binding.node_id.clone()));
    assert_eq!(
        i.stage_output("bindings", true).unwrap()[0]["binding"]["nodeId"],
        input.binding.node_id
    );
}

#[test]
fn pipeline_stays_with_original_owner_and_generation_after_refresh() {
    let mut e = traced_editor();
    let i = Inspection::capture(&e, None).unwrap();
    let old = i.stage_output("source", false).unwrap();
    let slot = e.fields.keys().next().unwrap().clone();
    e.edit(&slot, "private draft".into());
    let mut raw = i.received.clone();
    raw["pipeline"]["stages"][1]["output"]["text"] = json!("new generation");
    let p = Presentation::decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let generation = e.request(p.context.clone()).unwrap();
    e.receive(generation, p).unwrap();
    e.id = uuid::Uuid::new_v4();
    assert_eq!(i.stage_output("source", false).unwrap(), old);
    assert_ne!(i.owner, e.id);
    assert_ne!(i.generation, e.mounted_generation);
    assert!(!i.source.contains("private draft"));
}

#[test]
fn pipeline_absence_and_unknown_version_are_explicit() {
    let (e, _) = received();
    let i = Inspection::capture(&e, None).unwrap();
    assert!(
        i.stages()
            .unwrap_err()
            .to_string()
            .contains("No pipeline captured")
    );
    let mut i = Inspection::capture(&traced_editor(), None).unwrap();
    i.received["pipeline"]["version"] = json!(999);
    assert!(
        i.stages()
            .unwrap_err()
            .to_string()
            .contains("Unsupported pipeline version")
    );
    // Unknown data remains inspectable as raw JSON, without being interpreted.
    assert_eq!(i.received["pipeline"]["version"], 999);
}

#[test]
fn nested_component_uses_its_own_pipeline_not_the_outer_one() {
    let mut i = Inspection::capture(&traced_editor(), None).unwrap();
    let mut child = i.received.clone();
    child["display"]["nodes"][0]["id"] = json!("nested-node");
    child["pipeline"]["stages"][1]["output"]["text"] = json!("nested source");
    i.received["description"] = json!([{"kind":"patch","contents":[{"content":[child]}]}]);
    i.select(Some("nested-node".into()));
    assert_eq!(
        i.stage_output("source", false).unwrap()["text"],
        "nested source"
    );
    assert_eq!(
        i.component("nested-node").unwrap()["node"]["id"],
        "nested-node"
    );
}

#[test]
fn real_server_pipeline_preserves_prepared_output_and_input_identity() {
    let p = Presentation::decode(include_bytes!("fixtures/live-pipeline.json")).unwrap();
    let input = p
        .display
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.kind == "input")
        .unwrap()
        .id
        .clone();
    let mut e = Editor::new(p.snapshot.subject.clone(), p.view.clone());
    let ticket = e.request(p.context.clone()).unwrap();
    e.receive(ticket, p).unwrap();
    let i = Inspection::capture(&e, Some(input.clone())).unwrap();
    assert_eq!(i.stages().unwrap().len(), 14);
    assert_eq!(
        i.stage_output("display", false).unwrap(),
        i.received["display"]
    );
    assert_eq!(
        i.stage_output("bindings", false).unwrap(),
        i.received["controls"]
    );
    assert_eq!(i.stage_output("html", false).unwrap(), i.received["html"]);
    for stage in [
        "normalized",
        "visible",
        "semantic",
        "layout",
        "display",
        "bindings",
    ] {
        assert_eq!(
            i.stage_output(stage, true)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1,
            "{stage}"
        );
    }
    assert_eq!(
        i.stage_output("prepared", true).unwrap()["lines"][0]["id"],
        input
    );
    assert!(i.stage_output("metrics", true).unwrap()["size"]["pref"].is_number());
}

#[test]
fn whole_view_stage_ignores_node_selection_and_keeps_all_fragments() {
    let mut i = Inspection::capture(&traced_editor(), None).unwrap();
    let expected = i.view_stage_output("display").unwrap();
    let input = i.received["controls"][0]["binding"]["nodeId"]
        .as_str()
        .unwrap()
        .to_owned();
    i.select(Some(input));
    assert_eq!(i.view_stage_output("display").unwrap(), expected);
    assert_eq!(expected, i.received["display"]);

    let first = i.received.clone();
    let mut second = first.clone();
    second["pipeline"]["stages"][1]["output"]["text"] = json!("second fragment");
    i.received = json!({"description":[first, {"children":[second]}]});
    let stages = i.view_stage_output("source").unwrap();
    assert_eq!(stages.as_array().unwrap().len(), 2);
    assert_eq!(stages[0]["fragment"], "/description/0");
    assert_eq!(stages[1]["fragment"], "/description/1/children/0");
    assert_eq!(stages[1]["output"]["text"], "second fragment");
    i.received["description"][1]["children"][0]["pipeline"]["version"] = json!(999);
    assert!(
        i.view_stage_output("source")
            .unwrap_err()
            .to_string()
            .contains("Unsupported pipeline version")
    );
}
