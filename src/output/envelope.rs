//! twg's JSON output envelope.
//!
//! Jira, Confluence, user and account commands print
//!
//! ```json
//! {"apiVersion": "v2", "command": "jira.workitem.query",
//!  "request": {...}, "data": ..., "pageInfo": {...}, "meta": {...}}
//! ```
//!
//! as twg 1.4.4 does: `command` is twg's dotted operation name (not always
//! the CLI path), `request` echoes the inputs, `pageInfo` usually only
//! appears when another page exists (see [`Envelope::page_info`] for the
//! exceptions), and `meta.resourceType` names what `data` holds.
//! Bitbucket commands print their payload without an envelope, also as twg
//! does. In JSONL mode each element of the main collection is printed on its
//! own line with a `_twg` locator (see [`jsonl_rows`]).

use serde_json::{Map, Value, json};

/// The output contract version twg reports.
pub const API_VERSION: &str = "v2";

/// Builder for one envelope.
pub struct Envelope {
    command: String,
    resource_type: String,
    request: Option<Value>,
    page_info: Option<Value>,
    meta: Map<String, Value>,
}

/// Start an envelope for twg operation `command` returning `resource_type`.
pub fn envelope(command: &str, resource_type: &str) -> Envelope {
    Envelope {
        command: command.into(),
        resource_type: resource_type.into(),
        request: None,
        page_info: None,
        meta: Map::new(),
    }
}

impl Envelope {
    /// Echo the command's inputs, as twg does. Null values are kept: twg
    /// prints options that were not given as `null`.
    pub fn request(mut self, request: Value) -> Self {
        self.request = Some(request);
        self
    }

    /// `pageInfo: {hasNextPage, nextCursor}` when there is a next page.
    pub fn next_cursor(mut self, cursor: Option<impl Into<String>>) -> Self {
        if let Some(c) = cursor {
            self.page_info = Some(json!({"hasNextPage": true, "nextCursor": c.into()}));
        }
        self
    }

    /// Set `pageInfo` as given. Offset-paged Jira lists (comments,
    /// changelog) print it on every page, the last one included.
    pub fn page_info(mut self, page_info: Value) -> Self {
        self.page_info = Some(page_info);
        self
    }

    /// Add a `meta` entry.
    pub fn meta(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.meta.insert(key.into(), value.into());
        self
    }

    /// Finish the envelope around `data`. A batch result (`{items, summary}`)
    /// also gets `meta.acquisition`, as in twg.
    pub fn data(mut self, data: Value) -> Value {
        if let Some(summary) = batch_summary(&data) {
            self.meta.insert("acquisition".into(), summary);
        }
        let mut meta = Map::new();
        meta.insert("resourceClass".into(), "entity".into());
        meta.insert("sourceMode".into(), "rest".into());
        meta.insert("resourceType".into(), self.resource_type.into());
        meta.extend(self.meta);
        let mut out = Map::new();
        out.insert("apiVersion".into(), API_VERSION.into());
        out.insert("command".into(), self.command.into());
        if let Some(r) = self.request {
            out.insert("request".into(), r);
        }
        out.insert("data".into(), data);
        if let Some(p) = self.page_info {
            out.insert("pageInfo".into(), p);
        }
        out.insert("meta".into(), Value::Object(meta));
        Value::Object(out)
    }
}

fn batch_summary(data: &Value) -> Option<Value> {
    let summary = data.get("summary")?;
    (data.get("items").is_some_and(Value::is_array) && summary.get("requested").is_some())
        .then(|| summary.clone())
}

/// Whether `v` is a twg success envelope.
pub fn is_envelope(v: &Value) -> bool {
    v.get("apiVersion").is_some() && v.get("data").is_some()
}

/// Keys whose array is the main collection of an envelope's `data` object,
/// in the order they are looked for.
const COLLECTIONS: &[&str] = &[
    "items",
    "issues",
    "projects",
    "boards",
    "sprints",
    "transitions",
    "fields",
    "results",
    "values",
    "issueTypes",
];

/// The lines `-o jsonl` prints for `v`.
///
/// - An envelope prints each element of its main collection (`data` itself
///   when it is an array, else the first of [`COLLECTIONS`] it has, else
///   `data` as a single row), each with `_twg: {command, resourceType,
///   collection, index}`. An empty collection prints nothing.
/// - An error envelope prints one line with `_twg: {collection: "root",
///   index: 0}`.
/// - Anything else (Bitbucket payloads) prints the elements of a top-level
///   array or of its `values`/`items` list, else the value itself.
pub fn jsonl_rows(v: &Value) -> Vec<Value> {
    if v.get("ok") == Some(&Value::Bool(false)) {
        let mut e = v.clone();
        e["_twg"] = json!({"collection": "root", "index": 0});
        return vec![e];
    }
    if !is_envelope(v) {
        let rows = v
            .as_array()
            .or_else(|| v["values"].as_array())
            .or_else(|| v["items"].as_array());
        return match rows {
            Some(r) => r.clone(),
            None => vec![v.clone()],
        };
    }
    let data = &v["data"];
    let (collection, rows): (&str, Vec<Value>) = match data {
        Value::Array(a) => ("root", a.clone()),
        Value::Object(m) => match COLLECTIONS
            .iter()
            .find(|k| m.get(**k).is_some_and(Value::is_array))
        {
            Some(k) => (k, m[*k].as_array().cloned().unwrap_or_default()),
            None => ("root", vec![data.clone()]),
        },
        other => ("root", vec![other.clone()]),
    };
    let locator = |i: usize| {
        json!({
            "command": v["command"],
            "resourceType": v["meta"]["resourceType"],
            "collection": collection,
            "index": i,
        })
    };
    rows.into_iter()
        .enumerate()
        .map(|(i, r)| match r {
            Value::Object(mut m) => {
                m.insert("_twg".into(), locator(i));
                Value::Object(m)
            }
            other => json!({"value": other, "_twg": locator(i)}),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_envelopes_in_twg_key_order() {
        let v = envelope("jira.workitem.query", "jira.workitem")
            .request(json!({"jql": "x"}))
            .next_cursor(Some("c2"))
            .data(json!({"issues": []}));
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "apiVersion",
                "command",
                "request",
                "data",
                "pageInfo",
                "meta"
            ]
        );
        assert_eq!(
            v["pageInfo"],
            json!({"hasNextPage": true, "nextCursor": "c2"})
        );
        assert_eq!(v["meta"]["resourceType"], "jira.workitem");
        let none = envelope("a.b", "t")
            .next_cursor(None::<String>)
            .data(json!([]));
        assert!(none.get("pageInfo").is_none() && none.get("request").is_none());
    }

    #[test]
    fn jsonl_rows_follow_the_main_collection() {
        let v = envelope("jira.workitem.query", "jira.workitem")
            .data(json!({"issues": [{"key": "A-1"}, {"key": "A-2"}]}));
        let rows = jsonl_rows(&v);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[1]["_twg"],
            json!({"command": "jira.workitem.query", "resourceType": "jira.workitem", "collection": "issues", "index": 1})
        );
        let root = envelope("user.get", "user").data(json!({"accountId": "x"}));
        assert_eq!(jsonl_rows(&root)[0]["_twg"]["collection"], "root");
        let empty = envelope("a", "t").data(json!({"issues": []}));
        assert!(jsonl_rows(&empty).is_empty());
        let err = json!({"ok": false, "error": {"code": "X"}});
        assert_eq!(
            jsonl_rows(&err)[0]["_twg"],
            json!({"collection": "root", "index": 0})
        );
        assert_eq!(jsonl_rows(&json!([{"id": 1}, {"id": 2}])).len(), 2);
    }

    #[test]
    fn batch_results_get_acquisition_meta() {
        let v = envelope("jira.workitem.get", "jira.workitem").data(json!({
            "items": [], "summary": {"requested": 2, "succeeded": 2, "failed": 0}
        }));
        assert_eq!(v["meta"]["acquisition"]["requested"], 2);
    }
}
