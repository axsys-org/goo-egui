use serde_json::{Value, json};
use shrine_egui::{model::Presentation, protobuf::decode};
// Absence and null have the same optional contract meaning. HTML is a separate
// renderer projection, intentionally not produced by native preparation.
fn normalize(mut v: Value) -> Value {
    match &mut v {
        Value::Object(m) => {
            m.remove("html");
            m.retain(|_, v| !v.is_null() && !v.as_array().is_some_and(Vec::is_empty));
            for v in m.values_mut() {
                *v = normalize(v.take());
            }
        }
        Value::Array(a) => {
            for v in a {
                *v = normalize(v.take());
            }
        }
        Value::Number(n) => return json!(n.as_f64().unwrap()),
        _ => (),
    }
    v
}
fn assert_contract(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            assert!(
                a.keys().eq(b.keys()),
                "keys at {path}: {:?} vs {:?}",
                a.keys(),
                b.keys()
            );
            for (k, v) in a {
                assert_contract(v, &b[k], &format!("{path}/{k}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_contract(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-10,
            "{path}: {a} vs {b}"
        ),
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn protoc_domain_fixtures_decode_directly_into_models() {
    for (wire, source) in [
        (
            include_bytes!("fixtures/protobuf/rich.pb").as_slice(),
            include_str!("fixtures/protobuf/rich.json"),
        ),
        (
            include_bytes!("fixtures/protobuf/view.pb").as_slice(),
            include_str!("fixtures/protobuf/view.json"),
        ),
    ] {
        let actual = normalize(decode(wire).unwrap());
        let expected = normalize(serde_json::from_str::<Value>(source).unwrap());
        assert_contract(&actual, &expected, "");
        let p = Presentation::decode(wire).unwrap();
        assert!(p.received_json.is_none(), "No eager JSON inspector tree");
        assert_eq!(p.received_protobuf.as_deref().unwrap(), wire);
        assert!(p.display.is_some());
    }
    let p = Presentation::decode(include_bytes!("fixtures/protobuf/view.pb")).unwrap();
    p.validate(&p.context, &p.snapshot.subject).unwrap();
}
#[test]
fn native_double_bits_are_preserved_and_nonfinite_geometry_is_rejected() {
    let fixture = include_bytes!("fixtures/protobuf/rich.pb");
    let p = Presentation::decode(fixture).unwrap();
    assert_eq!(p.context.width.to_bits(), 480f64.to_bits());
    let mut pattern = vec![9];
    pattern.extend(480f64.to_le_bytes());
    let at = fixture
        .windows(pattern.len())
        .position(|v| v == pattern)
        .unwrap();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut bad = fixture.to_vec();
        bad[at + 1..at + 9].copy_from_slice(&value.to_le_bytes());
        assert!(decode(&bad).is_err());
    }
}

#[test]
fn rejects_old_generic_protocol_and_malformed_domain_messages() {
    for bytes in [
        vec![],
        vec![8, 1, 18, 2, 16, 0],
        vec![8, 2],
        vec![8, 2, 58, 0],
        vec![8, 2, 58, 3, 10, 1, 255],
        vec![8, 2, 8, 2, 58, 2, 10, 0],
        vec![8, 2, 58, 4, 10, 0, 10, 0],
        vec![8, 2, 58, 2, 10, 0, 58, 2, 10, 0],
    ] {
        assert!(decode(&bytes).is_err(), "{bytes:?}");
    }
    let fixture = include_bytes!("fixtures/protobuf/view.pb");
    for n in 0..fixture.len() {
        assert!(decode(&fixture[..n]).is_err(), "prefix {n}");
    }
    assert!(decode(&vec![0; 8 * 1024 * 1024 + 1]).is_err());
}
#[test]
fn explicit_false_empty_and_unknown_extension_survive() {
    let mut wire = vec![8, 2, 58, 4, 10, 0, 16, 0];
    assert_eq!(decode(&wire).unwrap(), json!({"error":"","ok":false}));
    wire.extend([160, 6, 0]);
    assert_eq!(decode(&wire).unwrap(), json!({"error":"","ok":false}));
}
#[test]
#[ignore = "Read-only parity and timing against an explicitly opted-in disposable Reaver server"]
fn live_json_protobuf_parity() {
    use std::time::{Duration, Instant};
    assert_eq!(std::env::var("GOO_DISPOSABLE").unwrap(), "1");
    let base = std::env::var("GOO_TEST_SERVER").unwrap();
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap();
    let render = vec![
        ("target", "/0x11/app/ios_demo/note"),
        ("slot", "/sys/slot/content"),
        ("width", "480"),
        ("textScale", "1"),
        ("direction", "ltr"),
    ];
    let cases = [
        ("view", "/goo/render", true, render),
        (
            "deck",
            "/goo/render",
            true,
            vec![
                ("target", "/0x11/app/ios_demo/workspace"),
                ("view", "/0x11/gov/ios_demo/editors"),
                ("width", "700"),
                ("textScale", "1"),
                ("direction", "ltr"),
            ],
        ),
        (
            "source",
            "/goo/source",
            false,
            vec![("target", "/0x11/app/ios_demo/notes"), ("axis", "z")],
        ),
        (
            "tree",
            "/goo/source",
            false,
            vec![
                ("target", "/0x11/app/ios_demo/notes"),
                ("axis", "z"),
                ("topology", "tree"),
                ("paged", "1"),
            ],
        ),
        (
            "status",
            "/goo/status",
            false,
            vec![("target", "/0x11/app/ios_demo/note")],
        ),
        (
            "receipt",
            "/goo/operation-status",
            false,
            vec![
                ("target", "/0x11/app/ios_demo/note"),
                ("invocation", "no-such-invocation"),
            ],
        ),
    ];
    let mut results = vec![];
    for (name, path, post, fields) in cases {
        let fetch = |binary: bool| {
            let mut fields = fields.clone();
            if binary {
                fields.push(("wire", "protobuf-v2"));
            }
            let start = Instant::now();
            let r = if post {
                http.post(format!("{base}{path}")).form(&fields)
            } else {
                http.get(format!("{base}{path}")).query(&fields)
            }
            .send()
            .unwrap();
            assert!(r.status().is_success(), "{name}: {}", r.status());
            if binary {
                assert_eq!(r.headers()["content-type"], shrine_egui::protobuf::MIME);
            }
            let bytes = r.bytes().unwrap().to_vec();
            (bytes, start.elapsed().as_secs_f64() * 1000.)
        };
        // Warm both paths. Alternate order for five pairs; no mutation between reads.
        fetch(false);
        fetch(true);
        let mut json_times = vec![];
        let mut proto_times = vec![];
        let mut sizes = (0, 0);
        for round in 0..5 {
            let (j, p) = if round % 2 == 0 {
                (fetch(false), fetch(true))
            } else {
                let p = fetch(true);
                (fetch(false), p)
            };
            assert_contract(
                &normalize(serde_json::from_slice::<Value>(&j.0).unwrap()),
                &normalize(decode(&p.0).unwrap()),
                name,
            );
            sizes = (j.0.len(), p.0.len());
            json_times.push(j.1);
            proto_times.push(p.1);
            if round == 0 && name == "view" {
                let prepared = Presentation::decode(&p.0).unwrap();
                prepared
                    .validate(&prepared.context, &prepared.snapshot.subject)
                    .unwrap();
                std::fs::write("evidence/protobuf-live-view.pb", &p.0).unwrap();
            }
        }
        json_times.sort_by(f64::total_cmp);
        proto_times.sort_by(f64::total_cmp);
        results.push(
            json!({"case":name, "jsonBytes":sizes.0, "protobufBytes":sizes.1,
            "jsonMedianMs":json_times[2], "protobufMedianMs":proto_times[2], "samples":5}),
        );
    }
    for wire in ["future-v2", "protobuf-v2&wire=protobuf-v2"] {
        let r = http
            .post(format!("{base}/goo/act"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(format!("wire={wire}"))
            .send()
            .unwrap();
        assert_eq!(
            r.status().as_u16(),
            415,
            "Unsupported/duplicate format must fail before action admission"
        );
    }
    let report = serde_json::to_string_pretty(&results).unwrap();
    std::fs::write("evidence/protobuf-parity.json", &report).unwrap();
    println!("{report}");
}
