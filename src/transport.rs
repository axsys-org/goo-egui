use crate::model::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, time::Duration};
use uuid::Uuid;
#[derive(Clone, Debug)]
pub struct Connection {
    pub server: String,
    pub target: String,
    pub slot: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectionCapture {
    pub selection: String,
    pub incarnation: u32,
    pub revision: u64,
    pub member: Member,
}
/// Created once at activation; private fields prevent retry-time recapture.
#[derive(Clone, Debug)]
pub struct Invocation {
    id: String,
    body: String,
    inputs: BTreeMap<String, String>,
    target: String,
    become_slot: Option<String>,
}
impl Invocation {
    pub fn become_slot(&self) -> Option<&str> {
        self.become_slot.as_deref()
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn inputs(&self) -> &BTreeMap<String, String> {
        &self.inputs
    }
    pub fn target(&self) -> &str {
        &self.target
    }
    pub fn capture(
        p: &Presentation,
        c: &Control,
        inputs: BTreeMap<String, String>,
        selections: &BTreeMap<String, SelectionCapture>,
    ) -> Result<Self> {
        p.validate(&p.context, &p.snapshot.subject)?;
        ensure!(
            p.controls.contains(c)
                && c.binding.presented
                && ["invoke", "invoke-and-become"].contains(&c.binding.kind.as_str()),
            "Action not bound to this display"
        );
        ensure!(
            inputs.keys().eq(p.seeds().keys()) && inputs.values().all(|v| ascii(v)),
            "Input slots mismatch or unsupported non-ASCII text; draft retained"
        );
        for c in &p.controls {
            if let Some(n) = p
                .display
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|n| n.id == c.id)
                && let Some(edit) = &n.editing
            {
                edit.decode(
                    inputs
                        .get(&c.slot)
                        .ok_or_else(|| anyhow::anyhow!("Missing rich input"))?,
                )?;
            }
        }
        let id = Uuid::new_v4().to_string();
        let s = &p.snapshot;
        let mut fields = vec![
            ("wire".into(), "protobuf-v2".into()),
            ("target".into(), s.subject.clone()),
            ("subject".into(), s.target.clone()),
            ("view".into(), p.view.clone()),
            ("control".into(), c.binding.route.clone()),
            ("expected".into(), s.expected.clone()),
            ("displayed".into(), s.displayed.clone()),
            ("publication".into(), s.publication.clone()),
            ("invocation".into(), id.clone()),
        ];
        fields.extend(
            inputs
                .iter()
                .map(|(k, v)| (format!("input:{k}"), v.clone())),
        );
        for r in &c.binding.captures {
            let v = selections
                .get(&r.selection)
                .ok_or_else(|| anyhow::anyhow!("Missing selection {}", r.selection))?;
            ensure!(v.member.status == "available", "Unavailable selection");
            fields.push((format!("input:{}", r.slot), serde_json::to_string(v)?));
        }
        Ok(Self {
            id,
            body: form(fields),
            inputs,
            target: s.subject.clone(),
            become_slot: c.r#become.then(|| c.slot.clone()),
        })
    }
}
pub fn form(fields: impl IntoIterator<Item = (String, String)>) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish()
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Accepted,
    Rejected(String),
    Unknown(String),
}
#[derive(Clone)]
pub struct Client {
    base: String,
    http: reqwest::blocking::Client,
    render_timeout: Duration,
}
impl Client {
    pub fn server(&self) -> &str {
        &self.base
    }
    pub fn new(base: &str) -> Result<Self> {
        let mut client = Self::with_timeout(base, Duration::from_secs(20))?;
        // Main's cold P6 renderer can exceed the ordinary request deadline.
        // Read-only preparation has a separate budget; save outcomes keep theirs.
        client.render_timeout = Duration::from_secs(120);
        Ok(client)
    }
    pub fn with_timeout(base: &str, timeout: Duration) -> Result<Self> {
        let u = url::Url::parse(base)?;
        ensure!(
            ["http", "https"].contains(&u.scheme())
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
                && u.query().is_none()
                && u.fragment().is_none(),
            "Invalid server URL"
        );
        Ok(Self {
            base: base.trim_end_matches('/').into(),
            render_timeout: timeout,
            http: reqwest::blocking::Client::builder()
                .timeout(timeout)
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }
    fn read(r: reqwest::blocking::Response) -> Result<(u16, Vec<u8>)> {
        let status = r.status().as_u16();
        let mut data = vec![];
        r.take(8 * 1024 * 1024 + 1).read_to_end(&mut data)?;
        ensure!(data.len() <= 8 * 1024 * 1024, "Response exceeds 8 MiB");
        Ok((status, data))
    }
    fn read_proto(r: reqwest::blocking::Response) -> Result<(u16, Vec<u8>)> {
        if r.status().is_success() {
            ensure!(
                r.headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.split(';').next())
                    == Some(crate::protobuf::MIME),
                "Server did not return Goo protobuf"
            );
        }
        let success = r.status().is_success();
        let result = Self::read(r)?;
        ensure!(
            !success || !crate::protobuf::is_json(&result.1),
            "Expected binary protobuf response"
        );
        Ok(result)
    }
    pub fn render(
        &self,
        target: &str,
        selector: &str,
        value: &str,
        ctx: &Context,
    ) -> Result<Presentation> {
        ensure!(["view", "slot"].contains(&selector), "Invalid selector");
        let mut fields = vec![
            ("target".into(), target.into()),
            (selector.into(), value.into()),
            ("width".into(), ctx.width.to_string()),
            ("textScale".into(), ctx.text_scale.to_string()),
            ("direction".into(), ctx.direction.clone()),
        ];
        append_pipeline(&mut fields);
        let mut binary = !fields.iter().any(|(k, v)| k == "pipeline" && v == "1");
        if binary {
            fields.push(("wire".into(), "protobuf-v2".into()));
        }
        let body = form(fields.clone());
        let response = self
            .http
            .post(format!("{}/goo/render", self.base))
            .timeout(self.render_timeout)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()?;
        let (mut status, mut data) = if binary {
            Self::read_proto(response)?
        } else {
            Self::read(response)?
        };
        // Only this explicit domain capability response permits a read-only
        // JSON request. Actions and receipts never renegotiate or recapture.
        if binary && status == 415 && json_contract(&data) {
            fields.retain(|(key, _)| key != "wire");
            let response = self
                .http
                .post(format!("{}/goo/render", self.base))
                .timeout(self.render_timeout)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(form(fields))
                .send()?;
            (status, data) = Self::read(response)?;
            binary = false;
        }
        ensure!(
            (200..300).contains(&status),
            "Render HTTP {status}: {}",
            response_error(&data)
        );
        let p = Presentation::decode(&data)?;
        ensure!(
            p.context == *ctx && p.snapshot.subject == target,
            "Prepared context/subject differs from request"
        );
        ensure!(
            selector != "view" || p.view == value,
            "Prepared recipe differs from request"
        );
        if selector == "slot" {
            #[derive(Deserialize)]
            struct Slot {
                slot: String,
            }
            let slot: Slot = if binary {
                crate::protobuf::deserialize(&data)?
            } else {
                serde_json::from_slice(&data)?
            };
            ensure!(slot.slot == value, "Prepared slot differs from request");
        }
        Ok(p)
    }
    pub fn explorer_bytes(
        &self,
        endpoint: &str,
        post: bool,
        fields: Vec<(String, String)>,
    ) -> Result<Vec<u8>> {
        ensure!(
            endpoint.starts_with('/')
                && !endpoint.starts_with("//")
                && !endpoint.contains(['?', '#']),
            "Invalid relative endpoint"
        );
        let mut fields = fields;
        if endpoint == "/goo/render"
            || (endpoint == "/goo/source"
                && fields
                    .iter()
                    .any(|(k, v)| k == "presentations" && v == "pane-v1"))
        {
            append_pipeline(&mut fields);
        }
        let protobuf = matches!(
            endpoint,
            "/goo/render"
                | "/goo/source"
                | "/goo/status"
                | "/goo/operation-status"
                | "/goo/edit-value"
        ) && !fields
            .iter()
            .any(|(k, v)| (k == "pipeline" && v == "1") || (k == "workspace" && !v.is_empty()))
            && (endpoint != "/goo/render"
                || fields
                    .iter()
                    .any(|(k, v)| ["view", "slot", "role"].contains(&k.as_str()) && !v.is_empty()));
        if protobuf {
            fields.push(("wire".into(), "protobuf-v2".into()));
        }
        let url = format!("{}{}", self.base, endpoint);
        let request = if post {
            self.http
                .post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(form(fields))
        } else {
            self.http.get(url).query(&fields)
        };
        let response = request.timeout(self.render_timeout).send()?;
        let (status, data) = if protobuf {
            Self::read_proto(response)?
        } else {
            Self::read(response)?
        };
        ensure!(
            (200..300).contains(&status),
            "HTTP {status}: {}",
            response_error(&data)
        );
        Ok(data)
    }
    pub fn explorer_json(
        &self,
        endpoint: &str,
        post: bool,
        fields: Vec<(String, String)>,
    ) -> Result<serde_json::Value> {
        crate::protobuf::decode_value(&self.explorer_bytes(endpoint, post, fields)?)
    }
    pub fn source(&self, path: &str, axis: &str) -> Result<Source> {
        let (status, data) = Self::read_proto(
            self.http
                .get(format!("{}/goo/source", self.base))
                .query(&[("target", path), ("axis", axis), ("wire", "protobuf-v2")])
                .send()?,
        )?;
        ensure!((200..300).contains(&status), "Source HTTP {status}");
        crate::protobuf::deserialize(&data)
    }
    pub fn activate(&self, i: &Invocation) -> Outcome {
        let response = self
            .http
            .post(format!("{}/goo/act", self.base))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(i.body.clone())
            .send()
            .map_err(anyhow::Error::from)
            .and_then(Self::read_proto);
        match response {
            Ok((status, data)) => activation_outcome(status, &data),
            Err(e) => Outcome::Unknown(e.to_string()),
        }
    }

    pub fn status(&self, i: &Invocation) -> Outcome {
        let get = || -> Result<Outcome> {
            let (status, data) = Self::read_proto(
                self.http
                    .get(format!("{}/goo/operation-status", self.base))
                    .query(&[
                        ("target", i.target()),
                        ("invocation", i.id()),
                        ("wire", "protobuf-v2"),
                    ])
                    .send()?,
            )?;
            ensure!((200..300).contains(&status), "Receipt HTTP {status}");
            let v = crate::protobuf::decode(&data)?;
            ensure!(v["operationId"] == i.id(), "Mismatched receipt");
            Ok(match v["outcome"].as_str() {
                Some("accepted") => Outcome::Accepted,
                Some("rejected") => Outcome::Rejected(v.to_string()),
                _ => Outcome::Unknown("Receipt unresolved; retry the original invocation".into()),
            })
        };
        get().unwrap_or_else(|e| Outcome::Unknown(e.to_string()))
    }
}

fn append_pipeline(fields: &mut Vec<(String, String)>) {
    if crate::inspector::capture_pipeline() && !fields.iter().any(|(key, _)| key == "pipeline") {
        fields.push(("pipeline".into(), "1".into()));
    }
}

fn json_contract(data: &[u8]) -> bool {
    #[derive(Deserialize)]
    struct Capability {
        code: Option<String>,
        error: String,
    }
    crate::protobuf::deserialize::<Capability>(data).is_ok_and(|v| {
        v.code.as_deref() == Some("json-contract") && v.error == "Goo JSON-only contract"
    })
}

// Validation failures may be protobuf; HTTP/parser failures may still be text.
fn response_error(data: &[u8]) -> String {
    crate::protobuf::decode(data)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| String::from_utf8_lossy(data).into_owned())
}

fn activation_outcome(status: u16, data: &[u8]) -> Outcome {
    if (200..300).contains(&status) {
        match crate::protobuf::deserialize::<Snapshot>(data) {
            Ok(snapshot) if !snapshot.expected.is_empty() && !snapshot.publication.is_empty() => {
                Outcome::Accepted
            }
            _ => Outcome::Unknown("Malformed success response".into()),
        }
    } else if (400..500).contains(&status) {
        Outcome::Rejected(response_error(data))
    } else {
        Outcome::Unknown(format!("HTTP {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_contract_requires_the_explicit_typed_read_capability() {
        let why = b"Goo JSON-only contract";
        let code = b"json-contract";
        let mut body = vec![10, why.len() as u8];
        body.extend(why);
        let mut without = vec![8, 2, 58, body.len() as u8];
        without.extend(&body);
        assert!(!json_contract(&without));
        body.extend([34, code.len() as u8]);
        body.extend(code);
        let mut response = vec![8, 2, 58, body.len() as u8];
        response.extend(body);
        assert!(json_contract(&response));
        response[1] = 1;
        assert!(!json_contract(&response));
        assert!(!json_contract(b"Unsupported Goo wire format"));
    }
    #[test]
    fn malformed_success_remains_unknown_and_never_becomes_a_rejection() {
        for bytes in [b"{}".as_slice(), &[8, 1, 18], &[8, 1, 18, 2, 16, 0]] {
            assert!(matches!(
                activation_outcome(200, bytes),
                Outcome::Unknown(_)
            ));
        }
        assert!(matches!(
            activation_outcome(500, b"failure"),
            Outcome::Unknown(_)
        ));
    }
    #[test]
    fn rejected_binary_and_text_responses_keep_readable_diagnostics() {
        // Response(failure=Failure(error="no")), independently encoded bytes.
        assert_eq!(
            activation_outcome(409, &[8, 2, 58, 4, 10, 2, b'n', b'o']),
            Outcome::Rejected(r#"{"error":"no"}"#.into())
        );
        assert_eq!(
            activation_outcome(415, b"Unsupported Goo wire format"),
            Outcome::Rejected("Unsupported Goo wire format".into())
        );
    }
}
