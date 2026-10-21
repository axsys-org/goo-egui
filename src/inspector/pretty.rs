//! Diagnostic projections only: never used to interpret, lay out, or invoke Goo.
use serde_json::Value;
use std::collections::BTreeSet;

/// Compact, selectable text. Raw JSON remains the precision/complete-data view.
pub fn format_stage(stage: &str, value: &Value) -> String {
    let mut prefixes = BTreeSet::new();
    fn scan(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) => {
                if let Some((prefix, suffix)) = s.rsplit_once(':')
                    && prefix.starts_with('/')
                    && !suffix.is_empty()
                    && suffix.chars().all(|c| c.is_ascii_digit())
                {
                    out.insert(prefix.to_owned());
                }
            }
            Value::Object(m) => {
                for (key, v) in m {
                    scan(&Value::String(key.clone()), out);
                    scan(v, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|v| scan(v, out)),
            _ => (),
        }
    }
    scan(value, &mut prefixes);
    let mut p = Printer {
        prefix: (prefixes.len() == 1).then(|| prefixes.into_iter().next().unwrap()),
        out: String::new(),
    };
    if let Some(prefix) = &p.prefix {
        p.line(0, format!("Node IDs: #n = {prefix}:n\n"));
    }
    p.stage(stage, value);
    p.out
}

struct Printer {
    prefix: Option<String>,
    out: String,
}
impl Printer {
    fn line(&mut self, depth: usize, text: impl AsRef<str>) {
        self.out.push_str(&"  ".repeat(depth));
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }
    fn atom(&self, v: &Value) -> String {
        match v {
            Value::Null => "—".into(),
            Value::String(s) => {
                if let Some(prefix) = &self.prefix
                    && let Some(tail) = s.strip_prefix(&format!("{prefix}:"))
                    && !tail.is_empty()
                    && tail.chars().all(|c| c.is_ascii_digit())
                {
                    return format!("#{tail}");
                }
                // Keep strings on one line without concealing embedded controls.
                s.chars().flat_map(char::escape_debug).collect()
            }
            Value::Number(n) if n.is_f64() => {
                let n = n.as_f64().unwrap();
                let s = format!("{n:.3}");
                s.trim_end_matches('0').trim_end_matches('.').to_owned()
            }
            _ => v.to_string(),
        }
    }
    fn quoted(&self, v: &Value) -> String {
        // JSON escaping distinguishes literal newlines from backslash-n.
        v.as_str()
            .map(|s| serde_json::to_string(s).unwrap())
            .unwrap_or_else(|| self.atom(v))
    }
    fn fields(&self, v: &Value, keys: &[&str]) -> String {
        keys.iter()
            .filter_map(|k| v.get(*k).map(|v| format!("{k} {}", self.atom(v))))
            .collect::<Vec<_>>()
            .join("   ·   ")
    }
    fn details(&mut self, v: &Value, exclude: &[&str], depth: usize) {
        if let Some(m) = v.as_object() {
            for (key, v) in m {
                if !exclude.contains(&key.as_str()) {
                    self.property(key, v, depth);
                }
            }
        }
    }
    fn property(&mut self, key: &str, v: &Value, depth: usize) {
        match v {
            Value::Object(m) if !m.is_empty() => {
                if m.values().all(|v| !v.is_object() && !v.is_array()) {
                    let fields = m
                        .iter()
                        .map(|(k, v)| format!("{k} {}", self.atom(v)))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    self.line(depth, format!("{key}: {fields}"));
                } else {
                    self.line(depth, key);
                    self.details(v, &[], depth + 1);
                }
            }
            Value::Array(a) if !a.is_empty() => {
                self.line(depth, format!("{key} · {} entries", a.len()));
                for (i, v) in a.iter().enumerate() {
                    self.property(&format!("[{i}]"), v, depth + 1);
                }
            }
            _ => self.line(depth, format!("{key}: {}", self.atom(v))),
        }
    }
    fn stage(&mut self, name: &str, v: &Value) {
        if let Some(fragments) = v.as_array().filter(|a| {
            !a.is_empty()
                && a.iter()
                    .all(|f| f.get("fragment").is_some() && f.get("output").is_some())
        }) {
            for f in fragments {
                self.line(
                    0,
                    format!(
                        "FRAGMENT {} · {}",
                        self.atom(&f["fragment"]),
                        self.atom(&f["status"])
                    ),
                );
                self.stage(name, &f["output"]);
                if f["diagnostics"].as_array().is_some_and(|a| !a.is_empty()) {
                    self.property("Diagnostics", &f["diagnostics"], 0);
                }
                self.line(0, "");
            }
            return;
        }
        if v.is_null() {
            self.line(
                0,
                "No output supplied for this stage. See the stage status and diagnostics.",
            );
            return;
        }
        match name {
            "source" | "core" | "normalized" | "visible" | "semantic" => self.tree(v, 0),
            "layout" => self.layout(v, 0),
            "metrics" => self.metrics(v),
            "trace" => self.trace(v),
            "prepared" => self.prepared(v),
            "display" => self.display(v),
            "bindings" => self.bindings(v),
            // HTML is source text, never interpreted or changed by a formatter.
            "html" if v.is_string() => self.line(0, v.as_str().unwrap()),
            _ => self.property("Output", v, 0),
        }
    }
    fn tree(&mut self, v: &Value, depth: usize) {
        if let Some(a) = v.as_array() {
            for n in a {
                self.tree(n, depth);
            }
            return;
        }
        if v.get("constructor").is_none() {
            self.property("Node", v, depth);
            return;
        }
        let a = &v["attrs"];
        let kind = v["kind"]
            .as_str()
            .unwrap_or(v["constructor"].as_str().unwrap_or("node"));
        let mut title = kind.to_owned();
        if let Some(d) = v.get("direction") {
            title.push_str(&format!(" {}", self.atom(d)));
        }
        if let Some(s) = a.get("slot") {
            title.push_str(&format!(" [{}]", self.atom(s)));
        }
        if let Some(t) = v.get("text") {
            title.push_str(&format!("  {}", self.quoted(t)));
        }
        if let Some(id) = a.get("id") {
            title.push_str(&format!("  {}", self.atom(id)));
        }
        self.line(depth, title);
        self.details(
            v,
            &[
                "constructor",
                "kind",
                "direction",
                "text",
                "attrs",
                "children",
            ],
            depth + 1,
        );
        // Common bookkeeping is intentionally omitted; unusual attrs stay visible.
        self.details(a, &["slot", "id", "optional", "origin", "after"], depth + 1);
        if a["optional"].as_i64().is_some_and(|n| n != 0) {
            self.property("optional", &a["optional"], depth + 1);
        }
        if !a["after"].is_null() {
            self.property("after", &a["after"], depth + 1);
        }
        if let Some(children) = v["children"].as_array() {
            for child in children {
                self.tree(child, depth + 1);
            }
        }
    }
    fn metrics(&mut self, v: &Value) {
        if !v["sizes"].is_object() && v.get("size").is_none() {
            self.property("Metrics", v, 0);
            return;
        }
        self.line(
            0,
            format!("Available width: {} CSS px\n", self.atom(&v["width"])),
        );
        self.line(
            0,
            format!(
                "{:<12} {:>12} {:>12} {:>8}",
                "NODE", "MIN", "PREFERRED", "GROW"
            ),
        );
        let mut row = |id: &Value, size: &Value| {
            self.line(
                0,
                format!(
                    "{:<12} {:>12} {:>12} {:>8}",
                    self.atom(id),
                    self.atom(&size["min"]),
                    self.atom(&size["pref"]),
                    self.atom(&size["grow"])
                ),
            );
            self.details(size, &["min", "pref", "grow"], 1);
        };
        if let Some(sizes) = v["sizes"].as_object() {
            for (id, size) in sizes {
                row(&Value::String(id.clone()), size);
            }
        } else if v.get("size").is_some() {
            row(&v["id"], &v["size"]);
        }
        self.details(v, &["width", "sizes", "id", "size"], 0);
    }
    fn layout(&mut self, v: &Value, depth: usize) {
        if let Some(a) = v.as_array() {
            for n in a {
                self.layout(n, depth);
            }
            return;
        }
        if v.get("id").is_none() {
            self.property("Layout", v, depth);
            return;
        }
        self.line(
            depth,
            format!(
                "{}   {} × {} px   ·   cost {}",
                self.atom(&v["id"]),
                self.atom(&v["w"]),
                self.atom(&v["h"]),
                self.atom(&v["cost"])
            ),
        );
        self.details(v, &["id", "w", "h", "cost", "rows"], depth + 1);
        if let Some(rows) = v["rows"].as_array() {
            for (i, row) in rows.iter().enumerate() {
                self.line(
                    depth + 1,
                    format!(
                        "row {}   children {}..{}   ·   gap {}   ·   height {}",
                        i + 1,
                        self.atom(&row["start"]),
                        self.atom(&row["end"]),
                        self.atom(&row["gap"]),
                        self.atom(&row["h"])
                    ),
                );
                self.details(row, &["start", "end", "gap", "h", "children"], depth + 2);
                if let Some(children) = row["children"].as_array() {
                    for n in children {
                        self.layout(n, depth + 2);
                    }
                }
            }
        }
    }
    fn trace(&mut self, v: &Value) {
        let Some(a) = v.as_array() else {
            self.property("Trace", v, 0);
            return;
        };
        self.line(0, format!("{} layout decisions\n", a.len()));
        for n in a {
            if !n.is_object() {
                self.property("Decision", n, 0);
                continue;
            }
            self.line(
                0,
                format!(
                    "{} → {}   {} · {}",
                    self.atom(&n["leftId"]),
                    self.atom(&n["rightId"]),
                    self.atom(&n["bind"]),
                    self.atom(&n["source"])
                ),
            );
            self.line(
                1,
                self.fields(
                    n,
                    &[
                        "axis",
                        "gap",
                        "preferredGap",
                        "stretch",
                        "breakPenalty",
                        "cut",
                    ],
                ),
            );
            self.line(1, self.fields(n, &["parent", "rule", "left", "right"]));
            self.details(
                n,
                &[
                    "leftId",
                    "rightId",
                    "bind",
                    "source",
                    "axis",
                    "gap",
                    "preferredGap",
                    "stretch",
                    "breakPenalty",
                    "cut",
                    "parent",
                    "rule",
                    "left",
                    "right",
                ],
                1,
            );
            self.line(0, "");
        }
    }
    fn prepared(&mut self, v: &Value) {
        if !v["lines"].is_array() {
            self.property("Prepared", v, 0);
            return;
        }
        self.property("Context", &v["context"], 0);
        self.line(0, "\nTEXT WRAPPING");
        if let Some(lines) = v["lines"].as_array() {
            for n in lines {
                self.line(
                    0,
                    format!(
                        "{}   natural width {} px",
                        self.atom(&n["id"]),
                        self.atom(&n["naturalWidth"])
                    ),
                );
                if let Some(lines) = n["lines"].as_array() {
                    for (i, l) in lines.iter().enumerate() {
                        self.line(1, format!("{}  {}", i + 1, self.quoted(l)));
                    }
                } else {
                    self.line(1, "No text lines");
                }
                self.details(n, &["id", "naturalWidth", "lines"], 1);
            }
        }
        if v.get("measurement").is_some() {
            self.line(0, "\nMeasurement bundle retained in Raw JSON; use metrics, layout and trace for its readable stages.");
        }
        self.details(v, &["context", "lines", "measurement"], 0);
    }
    fn display(&mut self, v: &Value) {
        let nodes = v.get("nodes").unwrap_or(v);
        if let Some(a) = nodes.as_array() {
            if v.is_object() {
                self.line(
                    0,
                    format!(
                        "{} nodes   ·   {} × {} CSS px\n",
                        a.len(),
                        self.atom(&v["width"]),
                        self.atom(&v["height"])
                    ),
                );
                self.property("Context", &v["context"], 0);
            }
            for n in a {
                if !n.is_object() {
                    self.property("Node", n, 0);
                    continue;
                }
                self.line(
                    0,
                    format!(
                        "\n{}  {} [{}]  {}",
                        self.atom(&n["id"]),
                        self.atom(&n["kind"]),
                        self.atom(&n["slot"]),
                        self.quoted(&n["payload"])
                    ),
                );
                self.line(
                    1,
                    format!(
                        "box  {}   ·   parent {}",
                        self.fields(&n["box"], &["x", "y", "width", "height"]),
                        self.atom(&n["parentId"])
                    ),
                );
                self.line(
                    1,
                    format!(
                        "font  {}",
                        self.fields(&n["textStyle"], &["fontId", "fontSize", "lineHeight"])
                    ),
                );
                self.line(
                    1,
                    format!(
                        "paint  foreground {}   ·   background {}",
                        self.atom(&n["paint"]["foreground"]),
                        self.atom(&n["paint"]["background"])
                    ),
                );
                self.property("semantics", &n["semantics"], 1);
                if let Some(lines) = n["lines"].as_array() {
                    for l in lines {
                        self.line(
                            1,
                            format!(
                                "baseline {}  {}",
                                self.atom(&l["baseline"]),
                                self.quoted(&l["text"])
                            ),
                        );
                    }
                }
                self.details(
                    n,
                    &[
                        "id",
                        "kind",
                        "slot",
                        "payload",
                        "parentId",
                        "box",
                        "textStyle",
                        "paint",
                        "semantics",
                        "lines",
                        "role",
                        "config",
                    ],
                    1,
                );
                if n["config"].as_object().is_some_and(|m| !m.is_empty()) {
                    self.property("config", &n["config"], 1);
                }
            }
            self.line(
                0,
                "\nFull paint recipes, font identities and line geometry are retained in Raw JSON.",
            );
            self.details(
                v,
                &[
                    "nodes",
                    "width",
                    "height",
                    "context",
                    "profile",
                    "version",
                    "coordinates",
                ],
                0,
            );
        } else {
            self.property("Display", v, 0);
        }
    }
    fn bindings(&mut self, v: &Value) {
        let Some(a) = v.as_array() else {
            self.property("Bindings", v, 0);
            return;
        };
        self.line(0, format!("{} control bindings\n", a.len()));
        for n in a {
            let b = &n["binding"];
            if !b.is_object() {
                self.property("Control", n, 0);
                continue;
            }
            self.line(
                0,
                format!(
                    "{}  {} → {}",
                    self.atom(&b["nodeId"]),
                    self.atom(&b["kind"]),
                    self.atom(&b["slot"])
                ),
            );
            self.line(1, self.fields(b, &["route", "presented", "version"]));
            self.property("captures", &b["captures"], 1);
            self.details(
                b,
                &[
                    "nodeId",
                    "kind",
                    "slot",
                    "route",
                    "presented",
                    "version",
                    "captures",
                ],
                1,
            );
            self.line(1, self.fields(n, &["invoke", "become"]));
            // Preserve contradictory legacy metadata rather than hiding a bad join.
            for (legacy, bound) in [
                ("id", "nodeId"),
                ("control", "route"),
                ("slot", "slot"),
                ("captures", "captures"),
            ] {
                if n.get(legacy).is_some() && n[legacy] != b[bound] {
                    self.property(&format!("outer {legacy}"), &n[legacy], 1);
                }
            }
            self.details(
                n,
                &[
                    "binding", "id", "control", "slot", "captures", "invoke", "become", "kind",
                ],
                1,
            );
            self.line(0, "");
        }
        self.line(
            0,
            "Descriptive inspection only; these records do not grant invocation authority.",
        );
    }
}
