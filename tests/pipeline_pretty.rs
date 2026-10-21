use serde_json::{Value, json};
use shrine_egui::inspector::pretty::format_stage;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/live-pipeline.json")).unwrap()
}
fn stage(v: &Value, name: &str) -> Value {
    v["pipeline"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap()["output"]
        .clone()
}

#[test]
fn live_stages_are_compact_and_keep_their_diagnostic_facts() {
    let v = fixture();
    let source = format_stage("source", &stage(&v, "source"));
    assert!(source.contains("text [title]  \"Hello from Reaver\""));
    assert!(source.contains("input [input]"));
    assert_eq!(source.lines().count(), 14);
    let semantic = format_stage("semantic", &stage(&v, "semantic"));
    assert!(semantic.contains("#9"));
    assert!(semantic.contains("Node IDs: #n = /0x11/gov/ios_demo/editor:n"));
    assert!(semantic.contains("bind tight"));
    let metrics = format_stage("metrics", &stage(&v, "metrics"));
    assert!(metrics.contains("PREFERRED"));
    assert!(metrics.contains("480 CSS px"));
    let layout = format_stage("layout", &stage(&v, "layout"));
    assert!(layout.contains("480 × 362.92 px"));
    assert!(layout.contains("row 1   children 0..1"));
    let trace = format_stage("trace", &stage(&v, "trace"));
    assert!(trace.contains("#8 → #9   tight · exact"));
    assert!(trace.contains("cut false"));
    let prepared = format_stage("prepared", &stage(&v, "prepared"));
    assert!(prepared.contains("natural width 179.849 px"));
    assert!(prepared.contains("1  \"Hello from Reaver\""));
    let display = format_stage("display", &stage(&v, "display"));
    assert!(display.contains("14 nodes"));
    assert!(display.contains("baseline 20.983"));
    assert!(display.contains("fontId sans620"));
    let bindings = format_stage("bindings", &stage(&v, "bindings"));
    assert!(bindings.contains("#9  input → /sys/req"));
    assert!(bindings.contains("route 0.4.0.1"));
    assert!(bindings.contains("presented true"));
    assert!(format_stage("parse", &Value::Null).contains("No output supplied"));
    assert_eq!(
        format_stage("html", &stage(&v, "html")),
        format!("{}\n", stage(&v, "html").as_str().unwrap())
    );
    // Formatting never mutates the source or alters any exact floating-point values.
    assert_eq!(v, fixture());
}

#[test]
fn fragments_retain_paths_status_diagnostics_and_unambiguous_ids() {
    let fragments = json!([
        {"fragment":"/content/0", "status":"ok", "output":{"id":"/a:1", "w":10,"h":20,"cost":0,"rows":null}, "diagnostics":[]},
        {"fragment":"/content/1", "status":"not supplied", "output":null, "diagnostics":["capture missing"]},
        {"fragment":"/content/2", "status":"ok", "output":{"id":"/b:1", "w":30,"h":40,"cost":0,"rows":null}, "diagnostics":[]}
    ]);
    let text = format_stage("layout", &fragments);
    for expected in [
        "FRAGMENT /content/0 · ok",
        "FRAGMENT /content/1 · not supplied",
        "capture missing",
        "/a:1",
        "/b:1",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(!text.contains("#1"));
}

#[test]
fn strings_unknown_fields_and_bad_joins_are_not_silently_rewritten() {
    let value = json!({"constructor":"text", "attrs":{"slot":"body", "id":"/view:1"}, "text":"line\n\"quoted\"\\n /view:1", "future":{"feature": "preserved"}});
    let text = format_stage("source", &value);
    assert!(text.contains(r#""line\n\"quoted\"\\n /view:1""#));
    assert!(text.contains("future: feature preserved"));
    let text = format_stage(
        "bindings",
        &json!([{"id":"/view:2", "control":"old", "binding":{"nodeId":"/view:1", "route":"new", "presented":false, "captures":[{"selection":"saved"}]}}]),
    );
    assert!(text.contains("outer id: #2"));
    assert!(text.contains("outer control: old"));
    assert!(text.contains("presented false"));
    assert!(text.contains("selection saved"));
}

#[test]
fn unknown_stages_and_unrecognized_shapes_remain_inspectable() {
    for name in [
        "future", "source", "layout", "metrics", "trace", "prepared", "display", "bindings",
    ] {
        let text = format_stage(name, &json!({"unexpected": {"sentinel": "keep me"}}));
        assert!(text.contains("keep me"), "{name}: {text}");
    }
    let selected = json!({"id":"/view:3", "size":{"min":5, "pref":10, "grow":1}, "width":40});
    assert!(format_stage("metrics", &selected).contains("#3"));
    assert!(format_stage("bindings", &json!([])).contains("0 control bindings"));
}
