use shrine_egui::{explorer::Guard, model::*, session::Editor, transport::*};
use std::collections::BTreeMap;
#[test]
fn recipe_destinations_are_frozen_and_do_not_authorize_invoke() {
    let mut p: Presentation =
        serde_json::from_str(include_str!("fixtures/live-view.json")).unwrap();
    let index = p
        .controls
        .iter()
        .position(|c| c.binding.kind == "invoke")
        .unwrap();
    p.controls[index].r#become = true;
    p.controls[index].slot = "/sys/slot/summary".into();
    p.controls[index].binding.slot = "/sys/slot/summary".into();
    p.controls[index].binding.kind = "invoke-and-become".into();
    p.validate(&p.context, &p.snapshot.subject).unwrap();
    let i = Invocation::capture(&p, &p.controls[index], p.seeds(), &BTreeMap::new()).unwrap();
    p.controls[index].slot = "/elsewhere".into();
    assert_eq!(i.become_slot(), Some("/sys/slot/summary"));
    p.controls[index].slot = "/sys/slot/summary".into();
    p.controls[index].invoke = false;
    p.controls[index].binding.kind = "become".into();
    p.validate(&p.context, &p.snapshot.subject).unwrap();
    assert!(Invocation::capture(&p, &p.controls[index], p.seeds(), &BTreeMap::new()).is_err());
}
#[test]
#[ignore = "Uses disposable shared panes recipe fixture"]
fn live_recipe_change_scaled_rtl_and_later_draft_guard() {
    assert_eq!(std::env::var("GOO_DISPOSABLE").unwrap(), "1");
    let client = Client::new(&std::env::var("GOO_TEST_SERVER").unwrap()).unwrap();
    let target = "/0x11/app/panes/cards/a";
    let ctx = Context {
        width: 420.,
        text_scale: 2.,
        direction: "rtl".into(),
    };
    let p = client
        .render(target, "slot", "/sys/slot/content", &ctx)
        .unwrap();
    p.validate(&ctx, target).unwrap();
    let edit = p.controls.iter().find(|c| c.r#become && !c.invoke).unwrap();
    let p = client.render(target, "slot", &edit.slot, &ctx).unwrap();
    p.validate(&ctx, target).unwrap();
    std::fs::write(
        "evidence/surfaces-rtl-controls.json",
        serde_json::to_vec(&p.received_value().unwrap()).unwrap(),
    )
    .unwrap();
    let mut e = Editor::new(target.into(), "/sys/slot/edit".into());
    let t = e.request(ctx.clone()).unwrap();
    e.receive(t, p).unwrap();
    e.edit("/sys/req", "Recipe transition persisted".into());
    let c = e
        .presentation
        .as_ref()
        .unwrap()
        .controls
        .iter()
        .find(|c| c.binding.kind == "invoke-and-become")
        .unwrap()
        .clone();
    let i = e.activate(&c, &BTreeMap::new()).unwrap();
    assert_eq!(i.become_slot(), Some("/sys/slot/content"));
    e.edit("/sys/req", "Later typing must survive".into());
    assert_eq!(client.activate(&i), Outcome::Accepted);
    assert!(e.complete(i.id(), Outcome::Accepted));
    assert!(e.dirty());
    let guard = Guard::capture(4, std::iter::once(&e));
    assert!(guard.check(4, std::iter::once(&e), false).is_err());
    let reread = client
        .render(target, "slot", i.become_slot().unwrap(), &ctx)
        .unwrap();
    reread.validate(&ctx, target).unwrap();
    assert!(
        reread
            .display
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|n| n.payload == "Recipe transition persisted")
    );
    assert_eq!(e.fields["/sys/req"].value, "Later typing must survive");
    println!(
        "PASS actual become binding, invoke-and-become save, scale 2 RTL preparation, persisted reread, later-draft departure guard"
    );
}
