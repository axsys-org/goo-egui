use eframe::egui::*;
use serde_json::json;
use shrine_egui::{images, model::*, paint_geometry::*};
#[test]
fn gradient_hard_stop_and_shape_clip_preserve_geometry() {
    let b:Background=serde_json::from_value(json!({"kind":"linear-gradient","angleDegrees":90,"stops":[{"offset":0.25,"color":"#ff0000"},{"offset":0.25,"color":"#0000ff"}]})).unwrap();
    let r = Rect::from_min_size(Pos2::ZERO, vec2(100., 40.));
    let mesh = gradient(r, &b);
    assert!(
        mesh.vertices
            .iter()
            .all(|v| r.expand(0.0001).contains(v.pos))
    );
    assert!(
        mesh.vertices
            .iter()
            .any(|v| (v.pos.x - 25.).abs() < 0.01 && v.color == Color32::RED)
    );
    assert!(
        mesh.vertices
            .iter()
            .any(|v| (v.pos.x - 25.).abs() < 0.01 && v.color == Color32::BLUE)
    );
    let shape = outline(
        r,
        &Corners {
            kind: "ellipse".into(),
            radius: None,
        },
    );
    let cut = clip(&mesh, &shape);
    assert!(cut.vertices.iter().all(|v| contains(v.pos, &shape)));
    assert!(!contains(Pos2::ZERO, &shape));
}
#[test]
fn decode_shared_svg_image_and_bound_allocations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/portable-displays.json")).unwrap();
    let source = corpus
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "image")
        .unwrap()["display"]["nodes"][0]["src"]
        .as_str()
        .unwrap();
    let image = images::decode(source).unwrap();
    assert_eq!(image.size, [270, 100]);
    assert_eq!(image.pixels[0], Color32::from_rgb(129, 153, 119));
    assert!(images::decode("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='999999' height='999999'/%3E").is_err());
}
#[test]
fn paint_shared_readonly_corpus_at_supplied_context() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/portable-displays.json")).unwrap();
    let ctx = eframe::egui::Context::default();
    shrine_egui::painter::install(&ctx);
    let mut count = 0;
    for case in corpus.as_array().unwrap() {
        let mut p: Presentation =
            serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap();
        p.display = Some(serde_json::from_value(case["display"].clone()).unwrap());
        p.context = p.display.as_ref().unwrap().context.clone();
        p.controls.clear();
        if p.display
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|n| n.kind == "input")
        {
            continue;
        }
        p.validate(&p.context, &p.snapshot.subject).unwrap();
        let mut editor =
            shrine_egui::session::Editor::new(p.snapshot.subject.clone(), p.view.clone());
        let t = editor.request(p.context.clone()).unwrap();
        editor.receive(t, p).unwrap();
        for _ in 0..2 {
            let output = ctx.run(Default::default(), |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    shrine_egui::painter::surface(ui, &mut editor, &Default::default());
                });
            });
            let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
        }
        count += 1;
    }
    assert_eq!(count, 11);
}
