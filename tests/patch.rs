use serde_json::{Value, json};
use shrine_egui::{model::*, patch::*, transport::*};
use std::collections::BTreeMap;
fn fixture() -> Packet {
    Packet::admit(Presentation::decode(include_bytes!("fixtures/patch-board.json")).unwrap())
        .unwrap()
}
fn movement(p: &Packet) -> Value {
    let n = &p.scene["nodes"][0];
    json!({"kind":"move-nodes","nodes":[{"target":node_target(n),"baseOrigin":{"x":n["rect"]["x"],"y":n["rect"]["y"]},"proposedOrigin":{"x":n["rect"]["x"].as_f64().unwrap()+10.,"y":n["rect"]["y"]}}]})
}
#[test]
fn admits_portable_patch_and_freezes_offer() {
    let mut p = fixture();
    assert_eq!(p.bodies.len(), 7);
    let args = movement(&p);
    let i = p
        .invocation("move-nodes", args.clone(), &BTreeMap::new())
        .unwrap();
    let bytes = i.body().to_owned();
    p.root.snapshot.expected = "new".into();
    p.scene["nodes"][0]["rect"]["x"] = json!(999);
    assert_eq!(i.body(), bytes);
    assert!(p.check_args(&args).is_err());
    let form: BTreeMap<_, _> = url::form_urlencoded::parse(i.body().as_bytes())
        .into_owned()
        .collect();
    assert_eq!(
        serde_json::from_str::<Value>(&form["input:/0x11/gov/patch_demo/operation"]).unwrap(),
        args
    );
}
#[test]
fn rejects_patch_identity_geometry_and_binding_tampering() {
    for change in 0..6 {
        let mut raw: Value =
            serde_json::from_slice(include_bytes!("fixtures/patch-board.json")).unwrap();
        let p = &mut raw["description"][0]["children"][0];
        match change {
            0 => p["contentVersion"] = json!(2),
            1 => p["scene"]["nodes"][1]["key"] = json!("source"),
            2 => p["scene"]["nodes"][0]["rect"]["width"] = json!(-1),
            3 => p["offerBindings"][0]["control"]["binding"]["route"] = json!("fake"),
            4 => p["contents"][0]["content"][2]["key"] = json!("missing"),
            _ => p["contents"][0]["content"][1]["context"]["direction"] = json!("rtl"),
        };
        assert!(
            Packet::admit(Presentation::decode(&serde_json::to_vec(&raw).unwrap()).unwrap())
                .is_err(),
            "case {change}"
        );
    }
}
#[test]
fn patch_refresh_preserves_draft_and_rejects_old_or_replaced_encounters() {
    let mut old = Work::new(fixture()).unwrap();
    let e = old
        .editors
        .values_mut()
        .find(|e| !e.fields.is_empty())
        .unwrap();
    let slot = e.fields.keys().next().unwrap().clone();
    e.edit(&slot, "draft".into());
    let owner = e.id;
    let mut next = Work::new(fixture()).unwrap();
    next.merge(&old).unwrap();
    assert_eq!(
        next.editors
            .values()
            .find(|e| e.id == owner)
            .unwrap()
            .fields[&slot]
            .value,
        "draft"
    );
    next.packet.sequence -= 1;
    assert!(next.merge(&old).is_err());
    next.packet.sequence = old.packet.sequence;
    next.packet.scene["header"]["encounter"] = json!("replacement");
    assert!(next.merge(&old).is_err());
    assert!(!old.can_leave());
}
#[test]
fn native_patch_paints_prepared_bodies() {
    let ctx = eframe::egui::Context::default();
    shrine_egui::painter::install(&ctx);
    let mut work = Work::new(fixture()).unwrap();
    for _ in 0..2 {
        let output = ctx.run(Default::default(), |ctx| {
            eframe::egui::CentralPanel::default().show(ctx, |ui| work.ui(ui));
        });
        assert!(!output.shapes.is_empty());
    }
}
#[test]
#[ignore = "Mutates the opt-in disposable patch-demo"]
fn live_patch_operations() {
    assert_eq!(std::env::var("GOO_DISPOSABLE").unwrap(), "1");
    let client = Client::new(&std::env::var("GOO_TEST_SERVER").unwrap()).unwrap();
    let read = || {
        Packet::admit(
            client
                .render(
                    "/0x11/app/patch_demo/board",
                    "view",
                    "/0x11/gov/patch_demo/editor",
                    &Context::new(1000.),
                )
                .unwrap(),
        )
        .unwrap()
    };
    let mut p = read();
    if !p.scene["connections"].as_array().unwrap().is_empty() {
        let c = &p.scene["connections"][0];
        let i=p.invocation("disconnect",json!({"kind":"disconnect","connection":connection_target(c),"topologyRef":c["topologyRef"]}),&BTreeMap::new()).unwrap();
        assert_eq!(client.activate(&i), Outcome::Accepted);
        p = read();
    }
    let origin = p.scene["nodes"][0]["rect"]["x"].as_f64().unwrap();
    let i = p
        .invocation("move-nodes", movement(&p), &BTreeMap::new())
        .unwrap();
    assert_eq!(client.activate(&i), Outcome::Accepted);
    p = read();
    assert_eq!(p.scene["nodes"][0]["rect"]["x"], json!(origin + 10.));
    assert!(p.covered.contains(i.id()));
    let revision = p.root.snapshot.expected.clone();
    assert_eq!(client.activate(&i), Outcome::Accepted);
    assert_eq!(read().root.snapshot.expected, revision);
    let from = port_target(&p.scene["nodes"][0], &p.scene["nodes"][0]["ports"][0]);
    let to = port_target(&p.scene["nodes"][2], &p.scene["nodes"][2]["ports"][0]);
    let i = p
        .invocation(
            "connect",
            json!({"kind":"connect","from":from,"to":to}),
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(client.activate(&i), Outcome::Accepted);
    p = read();
    assert_eq!(p.scene["connections"].as_array().unwrap().len(), 1);
    let c = &p.scene["connections"][0];
    let to = port_target(&p.scene["nodes"][1], &p.scene["nodes"][1]["ports"][0]);
    let i=p.invocation("reconnect",json!({"kind":"reconnect","connection":connection_target(c),"topologyRef":c["topologyRef"],"endpoint":"to","to":to}),&BTreeMap::new()).unwrap();
    assert_eq!(client.activate(&i), Outcome::Accepted);
    p = read();
    assert_eq!(
        p.scene["connections"][0]["endpoints"][1]["port"]["node"],
        "transform"
    );
    let c = &p.scene["connections"][0];
    let i=p.invocation("disconnect",json!({"kind":"disconnect","connection":connection_target(c),"topologyRef":c["topologyRef"]}),&BTreeMap::new()).unwrap();
    assert_eq!(client.activate(&i), Outcome::Accepted);
    p = read();
    assert!(p.scene["connections"].as_array().unwrap().is_empty());
    std::fs::write(
        "evidence/patch-reread.json",
        p.root.received_json.as_ref().unwrap().as_bytes(),
    )
    .unwrap();
    println!(
        "PASS persisted move, connect, reconnect, disconnect, exact retry and explicit coverage"
    );
}
#[test]
#[ignore = "Requires disposable patch server and real reply delay proxy"]
fn live_unknown_patch_outcome() {
    assert_eq!(std::env::var("GOO_DISPOSABLE").unwrap(), "1");
    let client = Client::new(&std::env::var("GOO_TEST_SERVER").unwrap()).unwrap();
    let slow = Client::with_timeout(
        &std::env::var("GOO_TEST_PROXY").unwrap(),
        std::time::Duration::from_millis(300),
    )
    .unwrap();
    let read = || {
        Packet::admit(
            client
                .render(
                    "/0x11/app/patch_demo/board",
                    "view",
                    "/0x11/gov/patch_demo/editor",
                    &Context::new(1000.),
                )
                .unwrap(),
        )
        .unwrap()
    };
    let p = read();
    let invocation = p
        .invocation("move-nodes", movement(&p), &BTreeMap::new())
        .unwrap();
    let frozen = invocation.body().to_owned();
    let mut work = Work::new(p.clone()).unwrap();
    let body = p.offers["move-nodes"].0.clone();
    let e = work.editors.get_mut(&body).unwrap();
    let origin = e.id;
    e.pending = Some(invocation.clone());
    e.sending = true;
    assert!(matches!(slow.activate(&invocation), Outcome::Unknown(_)));
    let e = work.editors.get_mut(&body).unwrap();
    e.complete(invocation.id(), Outcome::Unknown("response lost".into()));
    e.suspend();
    assert!(!work.can_leave());
    assert_eq!(client.status(&invocation), Outcome::Accepted);
    let e = work.editors.get_mut(&body).unwrap();
    assert_eq!(e.id, origin);
    assert!(e.complete(invocation.id(), Outcome::Accepted));
    assert!(!e.can_act());
    let before = read();
    assert!(before.covered.contains(invocation.id()));
    assert_eq!(client.activate(&invocation), Outcome::Accepted);
    let after = read();
    assert_eq!(before.root.snapshot.expected, after.root.snapshot.expected);
    assert_eq!(frozen, invocation.body());
    println!(
        "PASS unknown patch outcome, hidden owner, receipt, coverage, exact retry with no second mutation"
    );
}

#[test]
fn replacement_occurrence_gets_new_owner_and_dirty_replacement_is_refused() {
    let mut old = Work::new(fixture()).unwrap();
    let key = old
        .editors
        .keys()
        .find(|k| k.starts_with("patch_node:source/"))
        .unwrap()
        .clone();
    let id = old.editors[&key].id;
    let mut replaced = fixture();
    replaced.scene["nodes"][0]["lifetime"] = json!("new-incarnation");
    let mut next = Work::new(replaced.clone()).unwrap();
    next.merge(&old).unwrap();
    assert_ne!(next.editors[&key].id, id);
    old.editors.get_mut(&key).unwrap().fields.insert(
        "/draft".into(),
        shrine_egui::session::Field {
            seed: "seed".into(),
            value: "working".into(),
        },
    );
    assert!(Work::new(replaced).unwrap().merge(&old).is_err());
    assert_eq!(old.editors[&key].id, id);
    assert_eq!(old.editors[&key].fields["/draft"].value, "working");
}

#[test]
fn snapping_obeys_declared_screen_tolerance_and_ignores_selected_peers() {
    use eframe::egui::vec2;
    let mut p = fixture();
    p.scene["policy"]["alignmentSnap"] = json!(false);
    p.scene["policy"]["grid"] = json!({"origin":{"x":0,"y":0},"step":{"x":10,"y":10}});
    let mut node = p.scene["nodes"][0].clone();
    node["rect"]["x"] = json!(0);
    node["rect"]["y"] = json!(0);
    assert_eq!(
        snap_delta(&p, &[node.clone()], vec2(8., 0.), 1.),
        vec2(10., 0.)
    );
    assert_eq!(snap_delta(&p, &[node], vec2(7., 0.), 4.), vec2(7., 0.));
}

#[test]
fn patch_inspection_joins_nested_body_controls_from_received_response() {
    let p = fixture();
    let mut owner =
        shrine_egui::session::Editor::new(p.root.snapshot.subject.clone(), p.root.view.clone());
    owner.presentation = Some(p.root.clone());
    let inspect = shrine_egui::inspector::Inspection::capture(&owner, None).unwrap();
    for body in p.bodies.values() {
        for control in body.controls.iter().filter(|c| c.binding.presented) {
            let component = inspect.component(&control.binding.node_id).unwrap();
            assert!(
                component["controls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["id"] == control.id)
            );
        }
    }
    assert_eq!(&*inspect.source, &**p.root.received_json.as_ref().unwrap());
}
