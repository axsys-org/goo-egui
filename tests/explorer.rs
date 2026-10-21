use serde_json::json;
use shrine_egui::{explorer::*, model::*, session::Editor, transport::Client};
fn page(source: &str, parent: Option<&str>, epoch: &str) -> Page {
    serde_json::from_value(json!({"id":source,"life":"1","type":"namespace-record","axis":"z","topology":"tree","incremental":true,"bounded":true,"epoch":epoch,"omitted":0,"parent":parent.unwrap_or(source),"next":null,"members":if parent.is_none(){vec![json!({"key":source,"subject":source,"source":source,"sourceLife":"1","life":"1","type":"namespace-record","expected":"3","status":"unavailable","axis":"z","topology":"tree","parent":null,"hasChildren":true})]}else{vec![]},"presentations":[]})).unwrap()
}
#[test]
fn page_admission_is_atomic_and_epoch_bound() {
    let tree = Tree::new("/a".into(), "z".into(), "/summary".into());
    let tree = tree.with_page(page("/a", None, "9"), None, None).unwrap();
    let mut malformed = page("/a", Some("/a"), "10");
    assert!(tree.with_page(malformed.clone(), Some("/a"), None).is_err());
    assert_eq!(tree.epoch, "9");
    malformed.epoch = "9".into();
    malformed.life = "2".into();
    assert!(tree.with_page(malformed, Some("/a"), None).is_err());
    assert_eq!(tree.members.len(), 1);
}
#[test]
fn page_rejects_duplicate_members_and_missing_prepared_content() {
    let mut p = page("/a", None, "9");
    p.members.push(p.members[0].clone());
    assert!(p.validate("/a", "z", "/summary", None, None).is_err());
    p.members.pop();
    p.members[0].member.status = "available".into();
    assert!(p.validate("/a", "z", "/summary", None, None).is_err());
}
#[test]
fn page_rejects_foreign_parent_and_unbounded_sources() {
    let mut p = page("/a", None, "9");
    p.parent = "/elsewhere".into();
    assert!(p.validate("/a", "z", "/summary", None, None).is_err());
    p.parent = "/a".into();
    p.bounded = false;
    assert!(p.validate("/a", "z", "/summary", None, None).is_err());
}
#[test]
fn guard_refusal_supersession_and_identity_replacement() {
    let mut e = Editor::new("/a".into(), "/content".into());
    let guard = Guard::capture(4, [&e].into_iter());
    assert!(guard.check(5, [&e].into_iter(), true).is_err());
    e.revision += 1;
    assert!(guard.check(4, [&e].into_iter(), true).is_err());
    e.revision -= 1;
    e.id = uuid::Uuid::new_v4();
    assert!(guard.check(4, [&e].into_iter(), true).is_err());
}
#[test]
fn guard_cannot_discard_composition() {
    let mut e = Editor::new("/a".into(), "/content".into());
    e.composing = true;
    let guard = Guard::capture(1, [&e].into_iter());
    assert!(guard.check(1, [&e].into_iter(), true).is_err());
}
#[test]
fn entry_requires_installed_session_and_known_host() {
    let mut entry:Entry=serde_json::from_value(json!({"version":1,"target":"/0x11/goo/ses/1","source":"/a","view":"/v","host":{"version":1,"kind":"workspace","declaration":"/v","navigation":{"kind":"workspace-session","sourceSlot":"/s"},"operations":[]}})).unwrap();
    entry.validate("/a").unwrap();
    entry.target = "/a".into();
    assert!(entry.validate("/a").is_err());
    entry.target = "/0x11/goo/ses/1".into();
    entry.host.navigation.kind = "workspace-subject".into();
    assert!(entry.validate("/a").is_err());
}
#[test]
#[ignore = "requires disposable Explorer server"]
fn live_workspace_contract() {
    let client =
        Client::new(&std::env::var("GOO_SERVER").unwrap_or("http://127.0.0.1:8164".into()))
            .unwrap();
    let entry = client
        .workspace_entry("/goo-inspector", "/0x11/app/ios_demo")
        .unwrap();
    let workspace = client.workspace(entry).unwrap();
    std::fs::write(
        "tests/fixtures/explorer-workspace.json",
        workspace.packet.received_json.as_ref().unwrap().as_ref(),
    )
    .unwrap();
    let tree = Tree::new(
        "/0x11/app/ios_demo".into(),
        "z".into(),
        "/sys/slot/inspector_summary".into(),
    );
    let page = client.tree_page(&tree, None, None).unwrap();
    std::fs::write(
        "tests/fixtures/explorer-page.json",
        serde_json::to_vec_pretty(&page).unwrap(),
    )
    .unwrap();
    let tree = tree.with_page(page, None, None).unwrap();
    assert!(tree.members.len() > 1);
    let p = client
        .render(
            "/0x11/app/ios_demo/note",
            "slot",
            "/sys/slot/synthetic",
            &Context::new(720.),
        )
        .unwrap();
    std::fs::write(
        "tests/fixtures/explorer-values.json",
        p.received_json.as_ref().unwrap().as_ref(),
    )
    .unwrap();
    p.validate(&Context::new(720.), "/0x11/app/ios_demo/note")
        .unwrap();
}

#[test]
#[ignore = "requires disposable server with --tree-fixture"]
fn live_paged_tree() {
    let client =
        Client::new(&std::env::var("GOO_SERVER").unwrap_or("http://127.0.0.1:8165".into()))
            .unwrap();
    let root = "/0x11/app/tree_pages/root";
    let tree = Tree::new(
        root.into(),
        "z".into(),
        "/sys/slot/inspector_summary".into(),
    );
    let tree = tree
        .with_page(client.tree_page(&tree, None, None).unwrap(), None, None)
        .unwrap();
    let wide = format!("{root}/wide");
    assert!(tree.members.contains_key(&wide));
    assert!(!tree.next.contains_key(&wide));
    let first = client.tree_page(&tree, Some(&wide), None).unwrap();
    assert_eq!(first.members.len(), 64);
    assert!(first.next.is_some());
    let tree = tree.with_page(first, Some(&wide), None).unwrap();
    let cursor = tree.next[&wide].clone().unwrap();
    let second = client.tree_page(&tree, Some(&wide), Some(&cursor)).unwrap();
    assert_eq!(second.members.len(), 16);
    assert!(second.next.is_none());
    let loaded = tree.with_page(second, Some(&wide), Some(&cursor)).unwrap();
    assert_eq!(
        loaded
            .members
            .values()
            .filter(|n| n.parent.as_deref() == Some(&wide))
            .count(),
        80
    );
    let branch = format!("{wide}/c000");
    let deep = format!("{branch}/deep");
    assert!(!loaded.members.contains_key(&deep));
    let deferred = loaded
        .with_page(
            client.tree_page(&loaded, Some(&branch), None).unwrap(),
            Some(&branch),
            None,
        )
        .unwrap();
    assert!(deferred.members.contains_key(&deep));
    let mut stale = loaded.clone();
    stale.epoch = "0".into();
    assert!(client.tree_page(&stale, Some(&wide), None).is_err());
    assert!(client.tree_page(&loaded, Some("/outside"), None).is_err());
    assert!(
        loaded
            .members
            .values()
            .any(|m| m.member.status == "unavailable")
    );
}
#[test]
fn readonly_values_keep_received_json_and_grant_no_actions() {
    let bytes = std::fs::read("tests/fixtures/explorer-values.json").unwrap();
    let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(raw.get("readOnlyDisplay").is_some());
    let p = Presentation::decode(&bytes).unwrap();
    p.validate(&p.context, &p.snapshot.subject).unwrap();
    assert!(p.controls.is_empty());
    assert_eq!(
        p.received_json.as_deref(),
        Some(std::str::from_utf8(&bytes).unwrap())
    );
    let mut forged = raw;
    forged["view"] = json!("/arbitrary");
    assert!(Presentation::decode(&serde_json::to_vec(&forged).unwrap()).is_err());
}
