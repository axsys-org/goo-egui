use serde_json::{Value, json};
use shrine_egui::{model::*, patch::*, session::Editor, transport::*};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

fn settle(work: &mut Work, until: impl Fn(&Work) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let ctx = eframe::egui::Context::default();
    while !until(work) {
        work.poll(&ctx);
        assert!(
            Instant::now() < deadline,
            "Timed out: {} / {:?}",
            work.notice,
            work.editors.values().map(|e| &e.status).collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "Mutates the opt-in disposable --embedded-patch fixture"]
fn live_embedded_owner_survives_refresh_unknown_save_and_refused_replacement() {
    assert_eq!(std::env::var("GOO_DISPOSABLE").unwrap(), "1");
    let client = Client::new(&std::env::var("GOO_TEST_SERVER").unwrap()).unwrap();
    let slow = Client::with_timeout(
        &std::env::var("GOO_TEST_PROXY").unwrap(),
        Duration::from_millis(250),
    )
    .unwrap();
    let board = "/0x11/app/patch_demo/board";
    let view = "/0x11/gov/patch_demo/editor";
    let scene_slot = "/0x11/gov/patch_demo/scene";
    let read = || {
        Packet::admit(
            client
                .render(board, "view", view, &Context::new(1000.))
                .unwrap(),
        )
        .unwrap()
    };
    let replace = |scene: &Value| {
        let p = client
            .render(
                board,
                "view",
                "/0x11/gov/patch_demo/fixture_scene",
                &Context::new(1000.),
            )
            .unwrap();
        let mut e = Editor::new(board.into(), p.view.clone());
        let t = e.request(p.context.clone()).unwrap();
        e.receive(t, p).unwrap();
        e.edit(scene_slot, serde_json::to_string(scene).unwrap());
        let c = e
            .presentation
            .as_ref()
            .unwrap()
            .controls
            .iter()
            .find(|c| c.invoke && c.binding.presented && c.kind == "action")
            .unwrap()
            .clone();
        let i = e.activate(&c, &BTreeMap::new()).unwrap();
        assert_eq!(client.activate(&i), Outcome::Accepted);
    };
    let initial = read();
    // The root's resolved scene includes projection metadata. Fixture mutation
    // starts from the persisted input seed, not a reconstructed renderer scene.
    let scene_view = client
        .render(
            board,
            "view",
            "/0x11/gov/patch_demo/fixture_scene",
            &Context::new(1000.),
        )
        .unwrap();
    let original: Value = serde_json::from_str(&scene_view.seeds()[scene_slot]).unwrap();
    let mut work = Work::new(initial).unwrap();
    let key = "patch_node:source/ref:note";
    assert!(work.packet.references.contains_key(key));
    work.bind(&client);
    settle(&mut work, Work::ready);
    let owner = work.editors[key].id;
    let draft = format!("embedded-save-{}", uuid::Uuid::new_v4());
    work.editors
        .get_mut(key)
        .unwrap()
        .edit("/sys/req", draft.clone());
    let ctx = eframe::egui::Context::default();
    work.refresh(&ctx);
    settle(&mut work, Work::ready);
    assert_eq!(work.editors[key].id, owner);
    assert_eq!(work.editors[key].fields["/sys/req"].value, draft);

    let control = work.editors[key]
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .iter()
        .find(|c| c.invoke && c.binding.presented && c.kind == "action")
        .unwrap()
        .clone();
    work.bind(&slow);
    work.activate_body(key, &control, &ctx).unwrap();
    let invocation = work.editors[key].pending.clone().unwrap();
    let frozen = invocation.body().to_owned();
    // Later typing must not change the frozen request or become clean on accept.
    let later = format!("{draft}-later");
    work.editors
        .get_mut(key)
        .unwrap()
        .edit("/sys/req", later.clone());
    settle(&mut work, |w| !w.editors[key].sending);
    assert!(work.editors[key].status.contains("Unknown"));
    assert_eq!(client.status(&invocation), Outcome::Accepted);
    for e in work.editors.values_mut() {
        e.suspend();
    }
    assert!(!work.can_leave());

    let mut replacement = original.clone();
    replacement["nodes"][0]["lifetime"] = json!("hardening-reincarnation");
    replace(&replacement);
    assert!(Work::new(read()).unwrap().merge(&work).is_err());
    assert_eq!(work.editors[key].id, owner);
    assert_eq!(work.editors[key].pending.as_ref().unwrap().body(), frozen);
    work.bind(&client);
    work.recover_body(key, true, &ctx).unwrap();
    settle(&mut work, |w| w.editors[key].pending.is_none());
    assert!(work.editors[key].suspended);
    assert_eq!(work.editors[key].id, owner);
    assert_eq!(work.editors[key].fields["/sys/req"].seed, draft);
    assert_eq!(work.editors[key].fields["/sys/req"].value, later);
    assert!(!work.editors[key].can_act());
    assert!(Work::new(read()).unwrap().merge(&work).is_err());

    // A valid complete projection with the node removed must also be refused.
    let mut removed = original.clone();
    removed["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| n["key"] != "source");
    removed["connections"] = json!([]);
    replace(&removed);
    let candidate = read();
    assert!(!candidate.references.contains_key(key));
    assert!(Work::new(candidate).unwrap().merge(&work).is_err());
    assert_eq!(work.editors[key].fields["/sys/req"].value, later);

    // A refused candidate does not retire the old owner. Restore the fixture,
    // finish any held refresh, resume, then refresh before enabling actions.
    replace(&original);
    settle(&mut work, |w| !w.loading);
    for e in work.editors.values_mut() {
        e.resume();
    }
    assert!(!work.ready());
    work.refresh(&ctx);
    settle(&mut work, Work::ready);
    assert_eq!(work.editors[key].id, owner);
    assert_eq!(work.editors[key].fields["/sys/req"].value, later);
    assert!(work.editors[key].dirty());
    let p = client
        .render(
            "/0x11/app/ios_demo/note",
            "slot",
            "/sys/slot/content",
            &Context::new(400.),
        )
        .unwrap();
    assert_eq!(p.seeds()["/sys/req"], draft);
    let revision = p.snapshot.expected;
    assert_eq!(client.activate(&invocation), Outcome::Accepted);
    let p = client
        .render(
            "/0x11/app/ios_demo/note",
            "slot",
            "/sys/slot/content",
            &Context::new(400.),
        )
        .unwrap();
    assert_eq!(p.snapshot.expected, revision);
    assert_eq!(invocation.body(), frozen);
    // Explicit discard permits replacement, whose owner must be new.
    for f in work.editors.get_mut(key).unwrap().fields.values_mut() {
        f.value = f.seed.clone();
    }
    replace(&replacement);
    let mut next = Work::new(read()).unwrap();
    next.merge(&work).unwrap();
    assert_ne!(next.editors[key].id, owner);
    assert!(next.editors[key].pending.is_none());
    replace(&original);
    println!(
        "PASS embedded draft refresh, unknown save, frozen body, hidden completion, dirty removal/replacement refusal, resumed refresh, persisted reread, exact retry, fresh incarnation"
    );
}
