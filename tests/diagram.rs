use eframe::egui::vec2;
use shrine_egui::diagram::*;
#[test]
fn diagram_preserves_supplied_sizes_and_rejects_bad_topology() {
    let nodes = vec![
        Item {
            key: "r".into(),
            parent: None,
            size: vec2(240., 70.),
        },
        Item {
            key: "a".into(),
            parent: Some("r".into()),
            size: vec2(240., 100.),
        },
        Item {
            key: "b".into(),
            parent: Some("r".into()),
            size: vec2(240., 30.),
        },
    ];
    let boxes = arrange(&nodes).unwrap();
    assert_eq!(boxes["a"].size(), nodes[1].size);
    assert!(!boxes["a"].intersects(boxes["b"]));
    assert!(boxes["a"].left() > boxes["r"].right());
    let mut invalid = nodes.clone();
    invalid[1].parent = Some("b".into());
    invalid[2].parent = Some("a".into());
    assert!(arrange(&invalid).is_err());
    invalid[2].parent = Some("missing".into());
    assert!(arrange(&invalid).is_err());
}
