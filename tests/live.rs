//! Opt-in; only the disposable demo created by scripts/verify-live.py is authorized.
use shrine_egui::{model::*, session::*, transport::*};
use std::{collections::BTreeMap, time::Duration};
#[test]
#[ignore = "Mutates an explicitly opted-in disposable Reaver fixture"]
fn real_reaver_roundtrip_delay_unknown_and_deck() {
    assert_eq!(std::env::var("GOO_DISPOSABLE").unwrap(), "1");
    let base = std::env::var("GOO_TEST_SERVER").unwrap();
    let proxy = std::env::var("GOO_TEST_PROXY").unwrap();
    let client = Client::new(&base).unwrap();
    let slow = Client::with_timeout(&proxy, Duration::from_millis(350)).unwrap();
    let ctx = Context::new(700.);
    let target = "/0x11/app/ios_demo/note";
    let p = client
        .render(target, "slot", "/sys/slot/content", &ctx)
        .unwrap();
    p.validate(&ctx, target).unwrap();
    let mut e = Editor::new(target.into(), "/sys/slot/content".into());
    let t = e.request(ctx.clone()).unwrap();
    e.receive(t, p).unwrap();
    e.edit("/sys/req", "Rust egui persisted roundtrip".into());
    let c = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .last()
        .unwrap()
        .clone();
    let i = e.activate(&c, &BTreeMap::new()).unwrap();
    assert_eq!(client.activate(&i), Outcome::Accepted);
    e.complete(i.id(), Outcome::Accepted);
    let reread = client
        .render(target, "slot", "/sys/slot/content", &ctx)
        .unwrap();
    assert_eq!(reread.seeds()["/sys/req"], "Rust egui persisted roundtrip");
    std::fs::write(
        "evidence/live-reread.json",
        serde_json::to_vec_pretty(&reread).unwrap(),
    )
    .unwrap();
    let t = e.request(ctx.clone()).unwrap();
    e.receive(t, reread).unwrap();
    e.edit("/sys/req", "dirty through resize".into());
    let narrow = Context::new(420.);
    let t = e.request(narrow.clone()).unwrap();
    e.receive(
        t,
        client
            .render(target, "slot", "/sys/slot/content", &narrow)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(e.fields["/sys/req"].value, "dirty through resize");
    // Server-declared independent tabs, not two hand-created views.
    let workspace = "/0x11/app/ios_demo/workspace";
    let p = client
        .render(workspace, "view", "/0x11/gov/ios_demo/editors", &ctx)
        .unwrap();
    p.validate_envelope(&ctx, workspace).unwrap();
    let tabs = tabs(&p).unwrap();
    assert_eq!(tabs.len(), 2);
    let mut sources = BTreeMap::new();
    for tab in &tabs {
        sources.insert(
            tab.source.clone(),
            client.source(&tab.source, &tab.axis).unwrap(),
        );
    }
    let mut s = Session::default();
    let plan = s
        .stage(Change::Deck {
            tabs,
            sources,
            basis: p.lifetime(),
            publication: p.snapshot.publication,
        })
        .unwrap();
    s.commit(plan, false).unwrap();
    for index in 0..2 {
        let pane = &s.panes[index];
        let plan = s
            .stage(Change::Select {
                pane: pane.id,
                member: pane.source.members[index].clone(),
            })
            .unwrap();
        s.commit(plan, false).unwrap();
        let id = s.panes[index].id;
        s.switch(id);
        let e = s.panes[index].editor.as_mut().unwrap();
        let t = e.request(ctx.clone()).unwrap();
        e.receive(t, client.render(&e.target, "slot", &e.slot, &ctx).unwrap())
            .unwrap();
        e.edit("/sys/req", format!("independent draft {index}"));
    }
    let a = s.panes[0].id;
    let b = s.panes[1].id;
    s.switch(a);
    let e = s.panes[0].editor.as_mut().unwrap();
    let t = e.request(ctx.clone()).unwrap();
    e.receive(t, client.render(&e.target, "slot", &e.slot, &ctx).unwrap())
        .unwrap();
    let c = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .last()
        .unwrap()
        .clone();
    let invocation = e.activate(&c, &BTreeMap::new()).unwrap();
    let owner = e.id;
    // Proxy forwards to real Reaver, then holds the real reply past the client timeout.
    let transport = slow.clone();
    let frozen = invocation.clone();
    let delayed = std::thread::spawn(move || transport.activate(&frozen));
    assert!(s.switch(b));
    let outcome = delayed.join().unwrap();
    assert!(matches!(outcome, Outcome::Unknown(_)), "{outcome:?}");
    s.editor_mut(owner)
        .unwrap()
        .complete(invocation.id(), outcome);
    assert_eq!(
        s.panes[1].editor.as_ref().unwrap().fields["/sys/req"].value,
        "independent draft 1"
    );
    let plan = s.stage(Change::Remove { pane: a }).unwrap();
    assert!(s.commit(plan, true).is_err());
    assert_eq!(client.status(&invocation), Outcome::Accepted);
    let before = client
        .render(
            "/0x11/app/ios_demo/notes/a",
            "slot",
            "/sys/slot/content",
            &ctx,
        )
        .unwrap();
    let exact = s.editor_mut(owner).unwrap().pending.clone().unwrap();
    assert_eq!(invocation.body(), exact.body());
    assert_eq!(client.activate(&exact), Outcome::Accepted);
    let after = client
        .render(
            "/0x11/app/ios_demo/notes/a",
            "slot",
            "/sys/slot/content",
            &ctx,
        )
        .unwrap();
    assert_eq!(
        before.snapshot.expected, after.snapshot.expected,
        "Retry must not execute the mutation twice"
    );
    assert_eq!(after.seeds()["/sys/req"], "independent draft 0");
    s.editor_mut(owner)
        .unwrap()
        .complete(invocation.id(), Outcome::Accepted);
    assert!(s.editor_mut(owner).unwrap().suspended);
    s.switch(a);
    assert!(!s.editor_mut(owner).unwrap().can_act());
    let t = s.editor_mut(owner).unwrap().request(ctx.clone()).unwrap();
    s.editor_mut(owner).unwrap().receive(t, after).unwrap();
    assert!(s.editor_mut(owner).unwrap().can_act());
    // A delayed old render is ignored after a newer preparation ticket.
    let delayed_client = Client::new(&proxy).unwrap();
    let context = ctx.clone();
    let worker = std::thread::spawn(move || {
        delayed_client
            .render(
                "/0x11/app/ios_demo/notes/a",
                "slot",
                "/sys/slot/content",
                &context,
            )
            .unwrap()
    });
    let e = s.editor_mut(owner).unwrap();
    let old = e.request(ctx.clone()).unwrap();
    let new = e.request(ctx.clone()).unwrap();
    let fresh = client.render(&e.target, "slot", &e.slot, &ctx).unwrap();
    assert!(e.receive(new, fresh).unwrap());
    assert!(!e.receive(old, worker.join().unwrap()).unwrap());
    let dirty = s
        .panes
        .iter()
        .find(|p| p.id == b)
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .values();
    s.panes.reverse();
    assert_eq!(
        s.panes
            .iter()
            .find(|p| p.id == b)
            .unwrap()
            .editor
            .as_ref()
            .unwrap()
            .values(),
        dirty
    );
    let plan = s.stage(Change::Remove { pane: b }).unwrap();
    assert!(s.commit(plan.clone(), false).is_err());
    s.refuse();
    assert!(s.commit(plan, true).is_err());
    let plan = s.stage(Change::Remove { pane: b }).unwrap();
    s.commit(plan, true).unwrap();
    println!(
        "PASS real prepared view, save/reread, dirty resize, declared deck, hidden completion, timeout unknown, receipt, byte-identical retry without second mutation, delayed stale render, guarded removal"
    );
}
