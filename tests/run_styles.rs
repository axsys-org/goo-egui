use eframe::egui::{self, *};
use serde_json::{Value, json};
use shrine_egui::model::Presentation;

fn raw(index: usize) -> Value {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/prepared-run-styles.json")).unwrap();
    let mut raw: Value = serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap();
    raw["display"] = fixtures[index]["display"].clone();
    raw["context"] = raw["display"]["context"].clone();
    raw["profile"] = raw["display"]["profile"].clone();
    raw["controls"] = json!([]);
    raw
}
fn decode(raw: &Value) -> anyhow::Result<Presentation> {
    let p = Presentation::decode(&serde_json::to_vec(raw)?)?;
    p.validate(&p.context, &p.snapshot.subject)?;
    Ok(p)
}
fn styled(raw: &mut Value) -> &mut Value {
    raw["display"]["nodes"][0]["lines"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .flat_map(|l| l["runs"].as_array_mut().unwrap().iter_mut())
        .find(|r| r.get("resolved").is_some())
        .unwrap()
}
fn linked(raw: &mut Value) -> &mut Value {
    raw["display"]["nodes"][0]["lines"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .flat_map(|l| l["runs"].as_array_mut().unwrap().iter_mut())
        .find(|r| r["resolved"]["semantics"]["role"] == "link")
        .unwrap()
}
#[test]
fn actual_foil_styles_validate_at_multiple_widths_scale_and_rtl() {
    for index in 0..3 {
        let p = decode(&raw(index)).unwrap();
        let n = &p.display.as_ref().unwrap().nodes[0];
        let fragments: Vec<_> = n
            .lines
            .iter()
            .flat_map(|l| l.runs.as_ref().unwrap())
            .collect();
        for id in ["sans750", "italic400", "italic750", "mono700"] {
            assert!(
                fragments.iter().any(|r| r
                    .resolved
                    .as_ref()
                    .is_some_and(|s| s.text_style.font_id == id)),
                "{id}"
            );
        }
        assert!(
            fragments
                .iter()
                .any(|r| r.resolved.as_ref().is_some_and(|s| s.underline.is_some()))
        );
        assert!(p.controls.is_empty());
    }
    let p = decode(&raw(0)).unwrap();
    let linked = p.display.unwrap().nodes[0]
        .lines
        .iter()
        .flat_map(|l| l.runs.as_ref().unwrap())
        .filter(|r| r.myth.get("/sys/link").is_some_and(|p| p["kind"] == "path"))
        .count();
    assert!(
        linked >= 2,
        "Internal link must wrap across multiple fragments"
    );
}
#[test]
fn rejects_missing_or_forged_resolved_styles_before_mounting() {
    let mut legacy = raw(0);
    legacy["display"]["version"] = json!(1);
    assert!(decode(&legacy).is_err());
    for field in ["resolved", "myth"] {
        let mut r = raw(0);
        styled(&mut r).as_object_mut().unwrap().remove(field);
        assert!(decode(&r).is_err());
    }
    for (field, value) in [("version", json!(999)), ("foreground", json!("broken"))] {
        let mut r = raw(0);
        styled(&mut r)["resolved"][field] = value;
        assert!(decode(&r).is_err());
    }
    let mut r = raw(0);
    styled(&mut r)["resolved"]["textStyle"]["fontSHA256"] = json!("wrong");
    assert!(decode(&r).is_err());
    let mut r = raw(0);
    linked(&mut r)["resolved"]["semantics"]["target"]["path"] = json!("/other");
    assert!(decode(&r).is_err());
    let mut r = raw(0);
    linked(&mut r)["resolved"]["highlight"]["width"] = json!(-1);
    assert!(decode(&r).is_err());
    let mut r = raw(0);
    styled(&mut r)["resolved"]["invoke"] = json!("save");
    assert!(decode(&r).is_err());
}
#[test]
fn painter_uses_supplied_link_fill_and_emits_no_navigation() {
    let mut r = raw(0);
    linked(&mut r)["resolved"]["highlight"]["fill"] = json!("#123456");
    let p = decode(&r).unwrap();
    let ctx = Context::default();
    shrine_egui::painter::install(&ctx);
    let out = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500., 500.))),
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                shrine_egui::painter::snapshot(ui, &p);
            });
        },
    );
    assert!(out.shapes.iter().any(|s| matches!(&s.shape,egui::epaint::Shape::Rect(rect) if rect.fill == Color32::from_rgb(0x12,0x34,0x56))));
    assert!(
        out.shapes
            .iter()
            .any(|s| matches!(&s.shape, egui::epaint::Shape::LineSegment { .. }))
    );
    assert!(
        out.platform_output
            .commands
            .iter()
            .all(|c| !matches!(c, egui::OutputCommand::OpenUrl(_)))
    );
}

#[test]
fn live_authored_view_retains_styles_and_only_declared_controls() {
    let raw: Value = serde_json::from_str(include_str!("fixtures/live-run-styles.json")).unwrap();
    let p = decode(&raw).unwrap();
    assert_eq!(p.controls.len(), 3);
    assert_eq!(p.display.as_ref().unwrap().version, 2);
    let styled = p
        .display
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|n| {
            n.lines.iter().any(|l| {
                l.runs
                    .as_ref()
                    .is_some_and(|runs| runs.iter().any(|r| r.resolved.is_some()))
            })
        })
        .unwrap();
    assert!(p.controls.iter().all(|c| c.binding.node_id != styled.id));
}
