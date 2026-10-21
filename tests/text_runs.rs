use serde_json::{Value, json};
use shrine_egui::model::Presentation;

fn run(text: &str) -> Value {
    json!({"myth": {}, "lang": {"en": {"txt": text}}})
}
fn fixture() -> Value {
    let mut p: Value = serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap();
    for n in p["display"]["nodes"].as_array_mut().unwrap() {
        let text = n["payload"].as_str().unwrap().to_owned();
        n["textRuns"] = json!([run(&text)]);
        for l in n["lines"].as_array_mut().unwrap() {
            let mut bounds = l["box"].clone();
            bounds["x"] = l["textX"].clone();
            bounds["width"] =
                json!(l["box"]["width"].as_f64().unwrap() - l["prefixAdvance"].as_f64().unwrap());
            l["runs"] = json!([{"myth": {}, "lang": "en", "text": l["text"], "box": bounds,
                "textX": l["textX"], "baseline": l["baseline"]}]);
        }
    }
    p
}
fn valid(p: &Value) -> bool {
    let Ok(p) = Presentation::decode(&serde_json::to_vec(p).unwrap()) else {
        return false;
    };
    p.validate(&p.context, &p.snapshot.subject).is_ok()
}
fn text_node(p: &mut Value) -> &mut Value {
    p["display"]["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["kind"] == "text")
        .unwrap()
}
#[test]
fn supports_legacy_and_english_runs_with_exact_projection() {
    let legacy = serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap();
    assert!(valid(&legacy));
    assert!(valid(&fixture()));
}
#[test]
fn rejects_unknown_language_and_descriptor_without_silent_fallback() {
    let mut p = fixture();
    text_node(&mut p)["textRuns"][0]["lang"] = json!({"jp": {"txt": "test"}});
    assert!(!valid(&p));
    let mut p = fixture();
    text_node(&mut p)["textRuns"][0]["myth"] = json!({"weight": 700});
    assert!(!valid(&p));
    let mut p = fixture();
    text_node(&mut p)["lines"][0]["runs"][0]["myth"] = json!({"invoke": "save"});
    assert!(!valid(&p));
}
#[test]
fn rejects_inconsistent_text_and_missing_fragment_geometry() {
    let mut p = fixture();
    text_node(&mut p)["textRuns"][0]["lang"]["en"]["txt"] = json!("wrong");
    assert!(!valid(&p));
    let mut p = fixture();
    text_node(&mut p)["lines"][0]["runs"][0]["text"] = json!("wrong");
    assert!(!valid(&p));
    let mut p = fixture();
    text_node(&mut p)["lines"][0]
        .as_object_mut()
        .unwrap()
        .remove("runs");
    assert!(!valid(&p));
}
#[test]
fn rejects_run_displacement_wrong_width_and_baseline() {
    for field in ["textX", "baseline"] {
        let mut p = fixture();
        text_node(&mut p)["lines"][0]["runs"][0][field] = json!(9999.);
        assert!(!valid(&p));
    }
    let mut p = fixture();
    text_node(&mut p)["lines"][0]["runs"][0]["box"]["width"] = json!(0.);
    assert!(!valid(&p));
}

#[test]
fn real_foil_fixture_paints_wrapped_runs_at_prepared_origins() {
    use eframe::egui::{self, *};
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/prepared-text-runs.json")).unwrap();
    let mut raw: Value = serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap();
    raw["display"] = fixture["display"].clone();
    raw["context"] = fixture["display"]["context"].clone();
    raw["profile"] = fixture["display"]["profile"].clone();
    raw["controls"] = json!([]);
    assert!(valid(&raw));
    let p = Presentation::decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let node = &p.display.as_ref().unwrap().nodes[0];
    assert_eq!(node.text_runs.as_ref().unwrap().len(), 2);
    assert!(node.lines.len() > 1);
    assert!(
        node.lines
            .iter()
            .any(|l| l.runs.as_ref().unwrap().len() > 1)
    );
    let ctx = egui::Context::default();
    shrine_egui::painter::install(&ctx);
    let output = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400., 300.))),
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                shrine_egui::painter::snapshot(ui, &p);
            });
        },
    );
    assert!(!output.shapes.is_empty());
}

#[test]
fn captured_live_response_preserves_run_contract_and_control_authority() {
    let raw: Value = serde_json::from_str(include_str!("fixtures/live-text-runs.json")).unwrap();
    assert!(valid(&raw));
    let p = Presentation::decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(
        p.display
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .all(|n| n.text_runs.is_some())
    );
    assert_eq!(p.controls.len(), 3);
    assert!(
        p.display
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.kind == "input")
            .unwrap()
            .lines
            .is_empty()
    );
}
