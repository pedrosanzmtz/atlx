//! atlx's JSON output has twg's shape: the same envelope, `command` and
//! data key paths, for the commands checked live against twg 1.4.4.
//!
//! `tests/fixtures/twg/*.json` are twg outputs and `tests/fixtures/twg/rest/`
//! the REST responses behind them, both made synthetic by
//! `scripts/twg-fixtures.sh` (values replaced, key names kept). The mock
//! server answers with the REST fixtures, and each atlx result must have
//! the same key paths and types as twg's fixture. `request` and `meta` are
//! left out: they echo twg's OAuth and Teamwork Graph setup.

mod common;

use common::{MockResponse, MockServer};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::process::Command;

fn fixture(rel: &str) -> Value {
    let path = format!("{}/tests/fixtures/twg/{rel}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap()
}

fn server() -> MockServer {
    MockServer::with_handler(|req| {
        let p = req.path.as_str();
        let file = if p == "/rest/api/3/myself" {
            "myself"
        } else if p == "/rest/api/3/project/search" {
            "project-search"
        } else if p.starts_with("/rest/api/3/project/") {
            "project"
        } else if p == "/rest/api/3/search/jql" {
            "search"
        } else if p.ends_with("/transitions") {
            "transitions"
        } else if p.ends_with("/issuetypes") {
            return MockResponse::json(200, json!({"issueTypes": [{"id": "1", "name": "Task"}]}));
        } else if p.contains("/createmeta/") {
            "createmeta"
        } else if p.ends_with("/comment") {
            "comments"
        } else if p.ends_with("/changelog") {
            "changelog"
        } else if p.ends_with("/editmeta") {
            "editmeta"
        } else if p.starts_with("/rest/api/3/issue/") {
            "issue"
        } else if p == "/rest/agile/1.0/board" {
            "board"
        } else if p == "/rest/api/3/issueLinkType" {
            "link-types"
        } else if p == "/rest/api/3/issuetype/project" {
            "issuetype-project"
        } else if p == "/rest/api/3/status" {
            "statuses"
        } else if p == "/rest/api/3/priority" {
            "priorities"
        } else if p == "/rest/api/3/user/search" {
            "user-search"
        } else {
            return MockResponse::json(404, json!({"errorMessages": [format!("no route {p}")]}));
        };
        MockResponse::json(200, fixture(&format!("rest/{file}.json")))
    })
}

fn atlx(srv: &MockServer, args: &[&str]) -> Value {
    let dir = std::env::temp_dir().join(format!("atlx-shapes-{}", std::process::id()));
    let o = Command::new(env!("CARGO_BIN_EXE_atlx"))
        .args(args)
        .args(["-o", "json"])
        .env("ATLX_CONFIG", dir.join("missing.toml"))
        .env("ATLX_SITE", srv.base_url())
        .env("ATLX_EMAIL", "me@example.com")
        .env("ATLX_TOKEN", "t")
        .env_remove("ATLX_OUTPUT")
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}

/// Every key path with its JSON type; array indexes fold to `[]`.
fn shape(v: &Value, prefix: &str, out: &mut BTreeSet<(String, &'static str)>) {
    let kind = match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    if !prefix.is_empty() {
        out.insert((prefix.to_string(), kind));
    }
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if prefix.is_empty() && (k == "request" || k == "meta") {
                    continue;
                }
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                shape(x, &p, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                shape(x, &format!("{prefix}.[]"), out);
            }
        }
        _ => {}
    }
}

/// Paths only one side has, and paths whose non-null types differ.
fn diff(twg: &Value, ours: &Value) -> Vec<String> {
    let (mut t, mut a) = (BTreeSet::new(), BTreeSet::new());
    shape(twg, "", &mut t);
    shape(ours, "", &mut a);
    let paths = |s: &BTreeSet<(String, &str)>| -> BTreeSet<String> {
        s.iter().map(|(p, _)| p.clone()).collect()
    };
    let (tp, ap) = (paths(&t), paths(&a));
    let mut out: Vec<String> = tp
        .difference(&ap)
        .map(|p| format!("only twg: {p}"))
        .collect();
    out.extend(ap.difference(&tp).map(|p| format!("only atlx: {p}")));
    for (p, kt) in &t {
        for (q, ka) in &a {
            if p == q && kt != ka && *kt != "null" && *ka != "null" {
                out.push(format!("type of {p}: twg {kt}, atlx {ka}"));
            }
        }
    }
    out
}

#[test]
fn jira_and_user_output_matches_twg_shapes() {
    let srv = server();
    let cases: &[(&str, &[&str])] = &[
        ("whoami", &["whoami"]),
        ("space-query", &["jira", "space", "query"]),
        ("space-get", &["jira", "space", "get", "PROJ"]),
        (
            "workitem-query",
            &["jira", "workitem", "query", "--jql", "project = PROJ"],
        ),
        ("workitem-get", &["jira", "workitem", "get", "PROJ-1"]),
        (
            "workitem-get-batch",
            &["jira", "workitem", "get", "PROJ-1", "PROJ-2"],
        ),
        (
            "transitions-query",
            &["jira", "workitem", "transitions", "query", "--id", "PROJ-1"],
        ),
        (
            "create-metadata",
            &[
                "jira",
                "workitem",
                "field",
                "create-metadata",
                "--space",
                "PROJ",
                "--type",
                "Task",
            ],
        ),
        ("link-types", &["jira", "workitem", "link-types", "query"]),
        ("board-query", &["jira", "board", "query"]),
        ("user-search", &["user", "search", "--name", "alice"]),
        (
            "comment-query",
            &[
                "jira",
                "workitem",
                "comment",
                "query",
                "--issue-id",
                "PROJ-1",
            ],
        ),
        (
            "changelog-query",
            &[
                "jira",
                "workitem",
                "changelog",
                "query",
                "--issue-id",
                "PROJ-1",
                "--first",
                "2",
            ],
        ),
        (
            "update-metadata",
            &[
                "jira",
                "workitem",
                "field",
                "update-metadata",
                "--id",
                "PROJ-1",
            ],
        ),
        (
            "types-query",
            &["jira", "workitem", "types", "query", "--project", "PROJ"],
        ),
        ("statuses-query", &["jira", "workitem", "statuses", "query"]),
        (
            "priorities-query",
            &["jira", "workitem", "priorities", "query"],
        ),
    ];
    let mut problems = Vec::new();
    for (name, args) in cases {
        let twg = fixture(&format!("{name}.json"));
        let ours = atlx(&srv, args);
        if twg["command"] != ours["command"] {
            problems.push(format!(
                "{name}: command twg {} atlx {}",
                twg["command"], ours["command"]
            ));
        }
        problems.extend(
            diff(&twg, &ours)
                .into_iter()
                .map(|d| format!("{name}: {d}")),
        );
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
