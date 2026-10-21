//! Version-one prepared contract. Roles/config never determine paint or authority.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub version: u32,
    pub revision: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub width: f64,
    pub text_scale: f64,
    pub direction: String,
}
impl Context {
    pub fn new(width: f64) -> Self {
        Self {
            width,
            text_scale: 1.,
            direction: "ltr".into(),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Box2 {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Box2 {
    pub fn valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite() && v.abs() < 1e6)
            && self.width >= 0.
            && self.height >= 0.
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    pub font_id: String,
    #[serde(rename = "fontSHA256")]
    pub font_sha256: String,
    pub family: String,
    pub weight: u32,
    pub style: String,
    pub font_size: f64,
    pub line_height: f64,
    pub letter_spacing: f64,
    pub inset_x: f64,
    pub inset_y: f64,
    pub kerning: bool,
    pub ligatures: bool,
}
/// Extensible per-run metadata. Never a receiver, binding, or action authority.
/// Wire projection of a Foil myth. The source type is a path-to-pail map.
pub type RunDescriptor = BTreeMap<String, serde_json::Value>;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    En { txt: String },
}
impl Language {
    pub fn text(&self) -> &str {
        match self {
            Self::En { txt } => txt,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextRun {
    pub myth: RunDescriptor,
    pub lang: Language,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunFragment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<ResolvedRun>,
    pub myth: RunDescriptor,
    pub lang: String,
    pub text: String,
    pub r#box: Box2,
    pub text_x: f64,
    pub baseline: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedRun {
    pub version: u32,
    pub text_style: TextStyle,
    pub foreground: String,
    pub highlight: Option<RunHighlight>,
    pub underline: Option<RunUnderline>,
    pub semantics: RunSemantics,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunHighlight {
    pub fill: String,
    pub stroke: String,
    pub width: f64,
    pub radius: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunUnderline {
    pub color: String,
    pub width: f64,
    pub y: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunSemantics {
    pub role: String,
    pub target: Option<RunLink>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum RunLink {
    #[serde(rename = "path")]
    Path { path: String },
    #[serde(rename = "weblink")]
    Weblink { href: String },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<Vec<RunFragment>>,
    pub r#box: Box2,
    pub text_x: f64,
    pub baseline: f64,
    pub prefix_advance: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Background {
    pub kind: String,
    pub color: Option<String>,
    pub angle_degrees: Option<f64>,
    #[serde(default)]
    pub stops: Vec<GradientStop>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GradientStop {
    pub offset: f64,
    pub color: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Border {
    pub width: f64,
    pub color: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Corners {
    pub kind: String,
    pub radius: Option<f64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Outline {
    pub width: f64,
    pub color: String,
    pub offset: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct States {
    pub highlight: Option<Outline>,
    pub focus: Option<Outline>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paint {
    pub version: u32,
    pub units: String,
    pub color_space: String,
    pub foreground: String,
    pub background: Background,
    pub border: Border,
    pub corners: Corners,
    pub clip: bool,
    pub states: States,
    pub marker: Option<serde_json::Value>,
    pub image: Option<serde_json::Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Semantics {
    pub version: u32,
    pub role: String,
    pub label: String,
    pub value: Option<String>,
    pub editable: Option<bool>,
    pub multiline: Option<bool>,
    pub range: Option<ProgressRange>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProgressRange {
    pub minimum: f64,
    pub maximum: f64,
    pub value: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: String,
    pub slot: String,
    pub payload: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_runs: Option<Vec<TextRun>>,
    pub src: Option<String>,
    pub r#box: Box2,
    pub text_style: TextStyle,
    pub lines: Vec<Line>,
    pub paint: Paint,
    pub semantics: Semantics,
}
fn validate_run_myth(myth: &RunDescriptor) -> Result<()> {
    for (slot, pail) in myth {
        let object = pail
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Invalid run pail"))?;
        ensure!(
            object.len() == 2 && object.contains_key("kind") && object.contains_key("value"),
            "Invalid run pail envelope"
        );
        let kind = pail["kind"].as_str().unwrap_or("");
        let value = &pail["value"];
        match slot.as_str() {
            "/sys/font" => ensure!(
                kind == "path"
                    && value
                        .as_str()
                        .is_some_and(|p| p.starts_with('/') && p.len() < 4096),
                "Invalid font reference"
            ),
            "/sys/modifiers" => {
                ensure!(kind == "modifiers", "Invalid modifier pail");
                let values = value
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("Invalid modifier list"))?;
                let mut seen = BTreeSet::new();
                for v in values {
                    let name = v.as_str().unwrap_or("");
                    ensure!(
                        matches!(name, "bold" | "italic" | "underline") && seen.insert(name),
                        "Unsupported or duplicate font modifier"
                    );
                }
            }
            "/sys/link" => {
                let v = value.as_str().unwrap_or("");
                ensure!(
                    v.len() <= 4096
                        && !v.chars().any(char::is_control)
                        && ((kind == "path" && v.starts_with('/'))
                            || (kind == "weblink"
                                && (v.starts_with("https://") || v.starts_with("http://"))
                                && !v.chars().any(char::is_whitespace))),
                    "Invalid run link"
                );
            }
            _ => bail!("Unsupported RunDescriptor property: {slot}"),
        }
    }
    Ok(())
}
fn validate_text_style(
    s: &TextStyle,
    fonts: &BTreeMap<&str, String>,
    manifest: &serde_json::Value,
    extension: &serde_json::Value,
) -> Result<()> {
    let face = manifest["fonts"]
        .get(&s.font_id)
        .unwrap_or(&extension["fonts"][&s.font_id]);
    ensure!(
        face["family"] == s.family && face["weight"] == s.weight && face["style"] == s.style,
        "Font metadata mismatch"
    );
    ensure!(
        fonts.get(s.font_id.as_str()) == Some(&s.font_sha256) && !s.kerning && !s.ligatures,
        "Unsupported font identity/shaping"
    );
    ensure!(
        [
            s.font_size,
            s.line_height,
            s.letter_spacing,
            s.inset_x,
            s.inset_y
        ]
        .iter()
        .all(|v| v.is_finite() && v.abs() < 10000.)
            && s.font_size > 0.
            && s.line_height >= 0.
            && s.inset_x >= 0.
            && s.inset_y >= 0.,
        "Invalid typography"
    );
    Ok(())
}
fn validate_runs(
    node: &Node,
    fonts: &BTreeMap<&str, String>,
    manifest: &serde_json::Value,
    extension: &serde_json::Value,
) -> Result<()> {
    if let Some(runs) = &node.text_runs {
        ensure!(runs.len() <= 10000, "Too many text runs");
        let plain: String = runs.iter().map(|r| r.lang.text()).collect();
        ensure!(plain == node.payload, "Text runs disagree with payload");
        for r in runs {
            validate_run_myth(&r.myth)?;
            ensure!(
                node.kind != "input" || r.myth.is_empty(),
                "Styled native editing is not supported"
            );
        }
    }
    for line in &node.lines {
        if let Some(runs) = &line.runs {
            ensure!(runs.len() <= 10000, "Too many prepared run fragments");
            let plain: String = runs.iter().map(|r| r.text.as_str()).collect();
            ensure!(plain == line.text, "Run fragments disagree with line text");
            let mut x = line.text_x;
            for run in runs {
                ensure!(run.lang == "en", "Unsupported language or RunDescriptor");
                ensure!(
                    run.r#box.valid()
                        && run.text_x.is_finite()
                        && run.baseline.is_finite()
                        && (run.text_x - x).abs() < 0.001
                        && (run.r#box.x - run.text_x).abs() < 0.001
                        && (run.r#box.y - line.r#box.y).abs() < 0.001
                        && (run.r#box.height - line.r#box.height).abs() < 0.001
                        && (run.baseline - line.baseline).abs() < 0.001,
                    "Invalid prepared run geometry"
                );
                validate_run_myth(&run.myth)?;
                if !run.myth.is_empty() {
                    ensure!(
                        node.text_runs
                            .as_ref()
                            .is_some_and(|source| source.iter().any(|r| r.myth == run.myth)),
                        "Fragment metadata has no source run"
                    );
                    ensure!(run.resolved.is_some(), "Missing resolved run style");
                }
                if let Some(r) = &run.resolved {
                    ensure!(
                        r.version == 1 && color_valid(&r.foreground),
                        "Unsupported resolved run style"
                    );
                    validate_text_style(&r.text_style, fonts, manifest, extension)?;
                    ensure!(
                        (r.text_style.line_height - line.r#box.height).abs() < 0.001
                            && r.text_style.inset_x == 0.
                            && r.text_style.inset_y == 0.,
                        "Invalid fragment typography geometry"
                    );
                    if let Some(h) = &r.highlight {
                        ensure!(
                            color_valid(&h.fill)
                                && color_valid(&h.stroke)
                                && h.width.is_finite()
                                && (0. ..=10.).contains(&h.width)
                                && h.radius.is_finite()
                                && (0. ..=100.).contains(&h.radius),
                            "Invalid run highlight"
                        );
                    }
                    if let Some(u) = &r.underline {
                        ensure!(
                            color_valid(&u.color)
                                && u.width.is_finite()
                                && (0. ..=10.).contains(&u.width)
                                && u.y.is_finite()
                                && u.y >= line.r#box.y
                                && u.y <= line.r#box.y + line.r#box.height,
                            "Invalid run underline"
                        );
                    }
                    match &r.semantics.target {
                        Some(target) => {
                            ensure!(
                                r.semantics.role == "link" && r.highlight.is_some(),
                                "Missing link semantics/highlight"
                            );
                            let (kind, value) = match target {
                                RunLink::Path { path } => ("path", path),
                                RunLink::Weblink { href } => ("weblink", href),
                            };
                            ensure!(
                                run.myth
                                    .get("/sys/link")
                                    .is_some_and(|p| p["kind"] == kind && p["value"] == *value),
                                "Link semantics disagree with source metadata"
                            );
                        }
                        None => ensure!(
                            r.semantics.role == "text" && !run.myth.contains_key("/sys/link"),
                            "Invalid text semantics"
                        ),
                    }
                }
                x += run.r#box.width;
            }
            ensure!(
                (x - line.text_x - (line.r#box.width - line.prefix_advance)).abs() < 0.001,
                "Run widths disagree with line geometry"
            );
        } else {
            ensure!(node.text_runs.is_none(), "Missing prepared run fragments");
        }
    }
    Ok(())
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Display {
    pub version: u32,
    pub profile: Profile,
    pub context: Context,
    pub coordinates: String,
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<Node>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Snapshot {
    pub life: String,
    pub subject: String,
    pub target: String,
    pub expected: String,
    pub displayed: String,
    pub publication: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CaptureRef {
    pub slot: String,
    pub selection: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub version: u32,
    pub kind: String,
    pub node_id: String,
    pub route: String,
    pub slot: String,
    pub captures: Vec<CaptureRef>,
    pub presented: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Control {
    pub id: String,
    pub kind: String,
    pub control: String,
    pub slot: String,
    pub invoke: bool,
    pub r#become: bool,
    pub captures: Vec<CaptureRef>,
    pub binding: Binding,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Presentation {
    /// Original HTTP response, retained verbatim for inspection (never action authority).
    #[serde(skip)]
    pub received_json: Option<std::sync::Arc<str>>,
    pub version: u32,
    pub ok: bool,
    pub profile: Profile,
    pub context: Context,
    pub snapshot: Snapshot,
    pub view: String,
    pub kind: String,
    pub display: Option<Display>,
    pub controls: Vec<Control>,
    pub description: Option<Vec<PaneNode>>,
    pub host: Option<serde_json::Value>,
}

pub const FONT_FILES: &[(&str, &[u8])] = &[
    ("sans400", include_bytes!("../assets/goo-Sans400.ttf")),
    ("sans520", include_bytes!("../assets/goo-Sans520.ttf")),
    ("sans550", include_bytes!("../assets/goo-Sans550.ttf")),
    ("sans620", include_bytes!("../assets/goo-Sans620.ttf")),
    ("sans650", include_bytes!("../assets/goo-Sans650.ttf")),
    ("sans750", include_bytes!("../assets/goo-Sans750.ttf")),
    ("mono400", include_bytes!("../assets/goo-Mono400.ttf")),
    ("italic400", include_bytes!("../assets/goo-Italic400.ttf")),
    ("italic520", include_bytes!("../assets/goo-italic520.ttf")),
    ("italic550", include_bytes!("../assets/goo-italic550.ttf")),
    ("italic620", include_bytes!("../assets/goo-italic620.ttf")),
    ("italic650", include_bytes!("../assets/goo-italic650.ttf")),
    ("italic750", include_bytes!("../assets/goo-italic750.ttf")),
    ("mono700", include_bytes!("../assets/goo-mono700.ttf")),
];
pub fn profile() -> Profile {
    serde_json::from_str(include_str!("../assets/profile.json")).unwrap()
}
pub fn color_valid(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn ascii(s: &str) -> bool {
    s.bytes().all(|b| matches!(b, 32..=126 | 9 | 10 | 13))
}
impl Presentation {
    pub fn decode(data: &[u8]) -> Result<Self> {
        let mut p: Self = serde_json::from_slice(data)?;
        let raw: serde_json::Value = serde_json::from_slice(data)?;
        if p.display.is_none() && raw.get("readOnlyDisplay").is_some() {
            ensure!(
                p.view == "/sys/slot/synthetic" && p.kind == "view" && p.controls.is_empty(),
                "Read-only Values cannot grant action authority"
            );
            p.display = Some(serde_json::from_value(raw["readOnlyDisplay"].clone())?);
        }
        p.received_json = Some(std::str::from_utf8(data)?.into());
        Ok(p)
    }
    pub fn lifetime(&self) -> String {
        format!(
            "{:?}",
            (
                &self.snapshot.subject,
                &self.snapshot.target,
                &self.snapshot.life,
                &self.snapshot.publication,
                &self.view
            )
        )
    }
    pub fn validate_envelope(&self, context: &Context, target: &str) -> Result<()> {
        ensure!(
            self.version == 1 && self.ok && self.profile == profile(),
            "Unsupported Goo version/profile"
        );
        ensure!(
            &self.context == context
                && context.width.is_finite()
                && context.width > 0.
                && context.width < 10000.
                && context.text_scale.is_finite()
                && (0.25..=4.).contains(&context.text_scale)
                && ["ltr", "rtl"].contains(&context.direction.as_str()),
            "Unsupported/mismatched context"
        );
        ensure!(
            self.snapshot.subject == target
                && [
                    &self.snapshot.life,
                    &self.snapshot.target,
                    &self.snapshot.expected,
                    &self.snapshot.displayed,
                    &self.snapshot.publication,
                    &self.view
                ]
                .iter()
                .all(|s| !s.is_empty()),
            "Mismatched target or missing snapshot evidence"
        );
        ensure!(
            self.host.is_none(),
            "Host operations/navigation are unsupported in this slice"
        );
        Ok(())
    }
    pub fn validate(&self, context: &Context, target: &str) -> Result<()> {
        self.validate_envelope(context, target)?;
        ensure!(self.kind == "view", "Unsupported surface: {}", self.kind);
        let d = self
            .display
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing prepared display"))?;
        ensure!(
            matches!(d.version, 1 | 2)
                && d.profile == self.profile
                && d.context == self.context
                && d.coordinates == "absolute-css-px",
            "Incompatible prepared display"
        );
        ensure!(
            [d.width, d.height]
                .iter()
                .all(|v| v.is_finite() && *v >= 0. && *v < 1e6)
                && (d.width - context.width).abs() < 1.
                && d.nodes.len() <= 10000,
            "Invalid surface geometry"
        );
        let fonts: BTreeMap<_, _> = FONT_FILES
            .iter()
            .map(|(id, data)| (*id, format!("{:x}", Sha256::digest(data))))
            .collect();
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../assets/profile.json"))?;
        let extension: serde_json::Value =
            serde_json::from_str(include_str!("../assets/run-fonts.json"))?;
        let mut seen = BTreeMap::new();
        for n in &d.nodes {
            ensure!(
                !n.id.is_empty()
                    && !seen.contains_key(&n.id)
                    && n.parent_id.as_ref().is_none_or(|p| seen.contains_key(p)),
                "Duplicate node or invalid preorder parent"
            );
            ensure!(n.r#box.valid(), "Invalid node geometry");
            let role = match n.kind.as_str() {
                "group" => "group",
                "text" | "code" => "text",
                "button" => "button",
                "input" => "textbox",
                "image" | "media" | "avatar" | "presence" => "image",
                "bar" => "progressbar",
                _ => bail!("Unsupported primitive: {}", n.kind),
            };
            validate_text_style(&n.text_style, &fonts, &manifest, &extension)?;
            ensure!(
                n.lines.iter().all(|l| l.r#box.valid()
                    && [l.text_x, l.baseline, l.prefix_advance]
                        .iter()
                        .all(|v| v.is_finite() && v.abs() < 1e6)
                    && l.prefix_advance >= 0.
                    && ascii(&l.text)),
                "Invalid explicit text lines"
            );
            ensure!(
                n.semantics.version == 1 && n.semantics.role == role,
                "Unsupported semantics"
            );
            if n.kind == "input" {
                ensure!(
                    n.lines.is_empty()
                        && n.semantics.value.is_some()
                        && n.semantics.editable == Some(true)
                        && n.semantics.multiline == Some(false),
                    "Unsupported editing semantics"
                );
            }
            ensure!(
                d.version == 2
                    || !n
                        .text_runs
                        .as_ref()
                        .is_some_and(|runs| runs.iter().any(|r| !r.myth.is_empty()))
                        && !n
                            .lines
                            .iter()
                            .flat_map(|l| l.runs.iter().flatten())
                            .any(|r| r.resolved.is_some()),
                "Styled runs require display version 2"
            );
            validate_runs(n, &fonts, &manifest, &extension)?;
            let p = &n.paint;
            ensure!(
                p.version == 1
                    && p.units == "css-px"
                    && p.color_space == "srgb"
                    && color_valid(&p.foreground)
                    && color_valid(&p.border.color)
                    && p.border.width.is_finite()
                    && (0. ..10000.).contains(&p.border.width),
                "Unsupported paint"
            );
            ensure!(
                p.corners.kind == "ellipse"
                    || (p.corners.kind == "radius"
                        && p.corners
                            .radius
                            .is_some_and(|r| r.is_finite() && (0. ..10000.).contains(&r))),
                "Invalid corners"
            );
            if let Some(m) = &p.marker {
                ensure!(
                    n.kind == "text"
                        && m["kind"] == "circle"
                        && m["color"] == "foreground"
                        && ["diameter", "gap"].iter().all(|k| m[k]
                            .as_f64()
                            .is_some_and(|v| v.is_finite() && (0. ..10000.).contains(&v))),
                    "Invalid marker"
                );
            }
            if let Some(image) = &p.image {
                ensure!(
                    role == "image" && image["fit"] == "cover" && image["clipToCorners"] == true,
                    "Unsupported image recipe"
                );
            }
            if let Some(src) = &n.src {
                ensure!(
                    p.image.is_some()
                        && src.len() <= 2_000_000
                        && (src.starts_with("data:image/")
                            || src.starts_with("https://")
                            || src.starts_with("http://")
                            || src.starts_with('/')),
                    "Unsupported image source"
                );
            }
            match p.background.kind.as_str() {
                "none" => {}
                "solid" => ensure!(
                    p.background.color.as_ref().is_some_and(|c| color_valid(c)),
                    "Invalid solid fill"
                ),
                "linear-gradient" => {
                    ensure!(
                        p.background.angle_degrees.is_some_and(f64::is_finite)
                            && (2..=64).contains(&p.background.stops.len()),
                        "Invalid gradient"
                    );
                    let mut last = 0.;
                    for stop in &p.background.stops {
                        ensure!(
                            stop.offset.is_finite()
                                && (last..=1.).contains(&stop.offset)
                                && color_valid(&stop.color),
                            "Invalid gradient stop"
                        );
                        last = stop.offset;
                    }
                }
                _ => bail!("Unsupported fill: {}", p.background.kind),
            }
            if let Some(r) = &n.semantics.range {
                ensure!(
                    role == "progressbar"
                        && [r.minimum, r.maximum, r.value]
                            .iter()
                            .all(|v| v.is_finite())
                        && r.minimum < r.maximum
                        && (r.minimum..=r.maximum).contains(&r.value),
                    "Invalid progress range"
                );
            }
            for o in [&p.states.highlight, &p.states.focus].into_iter().flatten() {
                ensure!(
                    o.width.is_finite()
                        && o.width >= 0.
                        && o.width < 10000.
                        && o.offset.is_finite()
                        && o.offset.abs() < 10000.
                        && color_valid(&o.color),
                    "Invalid outline"
                );
            }
            seen.insert(n.id.clone(), n);
        }
        let mut ids = BTreeSet::new();
        let mut slots = BTreeSet::new();
        for c in &self.controls {
            let b = &c.binding;
            let kind = if c.kind == "input" {
                "input"
            } else if c.invoke {
                if c.r#become {
                    "invoke-and-become"
                } else {
                    "invoke"
                }
            } else if c.r#become {
                "become"
            } else {
                "none"
            };
            ensure!(
                ["input", "action"].contains(&c.kind.as_str())
                    && ids.insert(&c.id)
                    && !c.control.is_empty()
                    && b.version == 1
                    && b.kind == kind
                    && b.node_id == c.id
                    && b.route == c.control
                    && b.slot == c.slot
                    && b.captures == c.captures
                    && b.presented == seen.contains_key(&c.id),
                "Invalid binding-to-node join"
            );
            ensure!(
                !c.r#become || crate::explorer::path(&c.slot),
                "Invalid become destination"
            );
            if c.kind == "input" {
                ensure!(
                    !c.slot.is_empty() && slots.insert(&c.slot),
                    "Duplicate input slot"
                );
            }
            if let Some(n) = seen.get(&c.id) {
                ensure!(
                    n.kind == if c.kind == "input" { "input" } else { "button" },
                    "Binding kind mismatch"
                );
            }
            let mut captures = BTreeSet::new();
            ensure!(
                c.captures.iter().all(|r| !r.slot.is_empty()
                    && !r.selection.is_empty()
                    && captures.insert(&r.slot)),
                "Invalid captures"
            );
        }
        ensure!(
            self.controls
                .iter()
                .flat_map(|c| &c.captures)
                .all(|r| !slots.contains(&r.slot)),
            "Capture/input slot collision"
        );
        ensure!(
            d.nodes
                .iter()
                .filter(|n| n.kind == "input")
                .all(|n| self
                    .controls
                    .iter()
                    .any(|c| c.id == n.id && c.binding.kind == "input" && c.binding.presented)),
            "Unbound input"
        );
        Ok(())
    }
    pub fn seeds(&self) -> BTreeMap<String, String> {
        self.controls
            .iter()
            .filter(|c| c.binding.kind == "input" && c.binding.presented)
            .map(|c| {
                let n = self
                    .display
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .find(|n| n.id == c.id)
                    .unwrap();
                (c.slot.clone(), n.semantics.value.clone().unwrap())
            })
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaneNode {
    pub kind: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub renderer: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub selection: String,
    #[serde(default)]
    pub slot: String,
    #[serde(default)]
    pub axis: String,
    #[serde(default)]
    pub topology: String,
    pub initial: Option<String>,
    #[serde(default)]
    pub children: Vec<PaneNode>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub key: String,
    pub subject: String,
    pub source: String,
    pub source_life: String,
    pub life: String,
    pub r#type: String,
    pub expected: String,
    pub status: String,
    pub axis: String,
}
impl Member {
    pub fn identity(&self) -> String {
        format!(
            "{:?}",
            (
                &self.key,
                &self.subject,
                &self.source,
                &self.source_life,
                &self.life,
                &self.axis
            )
        )
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Source {
    pub id: String,
    pub life: String,
    pub r#type: String,
    pub axis: String,
    pub topology: String,
    pub bounded: bool,
    pub members: Vec<Member>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub selection: String,
    pub source: String,
    pub axis: String,
    pub list_slot: String,
    pub detail_slot: String,
    pub list_key: String,
    pub follower_key: String,
    pub initial: Option<String>,
}
impl Source {
    pub fn validate(&self, t: &Tab) -> Result<()> {
        ensure!(
            self.id == t.source
                && self.axis == t.axis
                && self.topology == "children"
                && self.bounded
                && self.r#type == "namespace-record"
                && !self.life.is_empty()
                && self.members.len() <= 64,
            "Unsupported membership source"
        );
        let mut keys = BTreeSet::new();
        for m in &self.members {
            let prefix = format!("{}/", self.id.trim_end_matches('/'));
            let child = m.subject.strip_prefix(&prefix).unwrap_or("");
            ensure!(
                !child.is_empty()
                    && !child.contains('/')
                    && m.key == m.subject
                    && keys.insert(&m.key)
                    && m.source == self.id
                    && m.source_life == self.life
                    && m.axis == self.axis
                    && m.r#type == self.r#type
                    && !m.life.is_empty()
                    && !m.expected.is_empty()
                    && ["available", "unavailable"].contains(&m.status.as_str()),
                "Invalid source member"
            );
        }
        Ok(())
    }
}
pub fn tabs(p: &Presentation) -> Result<Vec<Tab>> {
    ensure!(
        p.kind == "pane" && p.display.is_none() && p.controls.is_empty(),
        "Expected pane envelope"
    );
    let nodes = p
        .description
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Missing pane description"))?;
    ensure!(
        nodes.len() == 1 && nodes[0].kind == "pane" && !nodes[0].key.is_empty(),
        "Expected keyed root"
    );
    let root = &nodes[0];
    let (scope, children) = if root.children.len() == 1 && root.children[0].kind == "deck" {
        let deck = &root.children[0];
        ensure!(!deck.key.is_empty(), "Unkeyed deck");
        (
            format!("{:?}", (&root.key, &deck.key)),
            deck.children.as_slice(),
        )
    } else {
        (root.key.clone(), nodes.as_slice())
    };
    ensure!(children.len() <= 8, "Deck limit is eight panes");
    let mut out = vec![];
    let mut keys = BTreeSet::new();
    for p in children {
        ensure!(
            p.kind == "pane" && !p.key.is_empty() && keys.insert(&p.key),
            "Duplicate/invalid pane key"
        );
        fn walk<'a>(n: &'a PaneNode, v: &mut Vec<&'a PaneNode>) {
            v.push(n);
            for c in &n.children {
                walk(c, v)
            }
        }
        let mut all = vec![];
        walk(p, &mut all);
        ensure!(
            all.len() == 5
                && all
                    .iter()
                    .all(|n| ["pane", "selection", "split", "topos", "follow"]
                        .contains(&n.kind.as_str())),
            "Only finite list/follower panes are supported; trees/patches are unsupported"
        );
        let get = |kind: &str| -> Result<&PaneNode> {
            let v: Vec<_> = all.iter().filter(|n| n.kind == kind).collect();
            ensure!(v.len() == 1, "Expected one {kind}");
            Ok(*v[0])
        };
        let s = get("selection")?;
        let l = get("topos")?;
        let f = get("follow")?;
        let mut scoped = vec![];
        walk(s, &mut scoped);
        ensure!(
            scoped.iter().any(|n| std::ptr::eq(*n, l))
                && scoped.iter().any(|n| std::ptr::eq(*n, f)),
            "Unscoped follower/list"
        );
        let axis = if s.axis.is_empty() { "z" } else { &s.axis };
        let laxis = if l.axis.is_empty() { "z" } else { &l.axis };
        ensure!(
            !s.key.is_empty()
                && !l.key.is_empty()
                && !f.key.is_empty()
                && s.source.starts_with('/')
                && l.source == s.source
                && l.selection == s.key
                && f.selection == s.key
                && l.renderer == "list"
                && ["", "children"].contains(&s.topology.as_str())
                && ["", "children"].contains(&l.topology.as_str())
                && ["x", "y", "z"].contains(&axis)
                && axis == laxis
                && !l.slot.is_empty()
                && !f.slot.is_empty(),
            "Unsupported pane binding"
        );
        out.push(Tab {
            id: format!("{scope}|{}", p.key),
            title: if p.label.is_empty() {
                p.key.rsplit('/').next().unwrap().into()
            } else {
                p.label.clone()
            },
            selection: s.key.clone(),
            source: s.source.clone(),
            axis: axis.into(),
            list_slot: l.slot.clone(),
            detail_slot: f.slot.clone(),
            list_key: l.key.clone(),
            follower_key: f.key.clone(),
            initial: s.initial.clone(),
        });
    }
    Ok(out)
}
