//! Jira Cloud REST v3 and Agile 1.0, shaped after twg's `jira` commands.

use crate::client::{ApiError, Client, ErrorKind};
use crate::markup::{TextFormat, quote, to_adf};
use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use serde_json::{Map, Value, json};

/// Fields `jira workitem get` fetches by default, as twg does.
pub const GET_FIELDS: &[&str] = &[
    "summary",
    "issuetype",
    "project",
    "status",
    "statusCategory",
    "priority",
    "assignee",
    "reporter",
    "creator",
    "created",
    "updated",
    "duedate",
    "resolution",
    "resolutiondate",
    "labels",
    "components",
    "fixVersions",
    "description",
    "subtasks",
    "issuelinks",
    "attachment",
    "comment",
    "worklog",
    "timetracking",
    "votes",
    "watches",
    "parent",
];

/// Fields behind the compact rows of `jira workitem query` and `search`.
const LIST_FIELDS: &[&str] = &[
    "summary",
    "status",
    "issuetype",
    "priority",
    "assignee",
    "reporter",
    "project",
    "created",
    "updated",
];

fn enc(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

fn browse_url(c: &Client, key: &str) -> String {
    format!("{}/browse/{key}", c.base())
}

/// Split twg's comma-separated list flags (`--labels a,b`).
pub fn csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect()
}

/// Merge `--fields-json` and repeated `--field KEY=VALUE` (values parse as JSON
/// when valid, else stay strings). The same key twice is an error, as in twg.
pub fn extra_fields(fields_json: Option<&str>, field: &[String]) -> Result<Map<String, Value>> {
    let mut out = match fields_json {
        Some(j) => {
            match serde_json::from_str::<Value>(j).context("--fields-json is not valid JSON")? {
                Value::Object(m) => m,
                _ => bail!("--fields-json must be a JSON object"),
            }
        }
        None => Map::new(),
    };
    for kv in field {
        let (k, v) = kv
            .split_once('=')
            .with_context(|| format!("--field '{kv}' must be KEY=VALUE"))?;
        if out.contains_key(k) {
            bail!("field '{k}' given more than once (via --field and/or --fields-json)");
        }
        let v = serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.to_string()));
        out.insert(k.to_string(), v);
    }
    Ok(out)
}

/// The host name of the client's site, as twg echoes it in `request.site`.
pub fn site(c: &Client) -> String {
    let b = c.base();
    b.split_once("://")
        .map_or(b, |(_, h)| h)
        .trim_end_matches('/')
        .to_string()
}

/// Remove avatar and icon URLs at any depth, as twg does in JSON output.
pub fn strip_media(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.shift_remove("avatarUrls");
            m.shift_remove("iconUrl");
            m.values_mut().for_each(strip_media);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_media),
        _ => {}
    }
}

/// A REST issue with its fields lifted to the top level next to `id`,
/// `key` and `self`, plus the browse `url`: twg's work item shape.
pub fn flatten(c: &Client, issue: &Value) -> Value {
    let mut out = Map::new();
    for k in ["expand", "id", "self", "key"] {
        if let Some(v) = issue.get(k) {
            out.insert(k.into(), v.clone());
        }
    }
    if let Some(f) = issue["fields"].as_object() {
        out.extend(f.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    let key = issue["key"].as_str().unwrap_or_default();
    out.insert("url".into(), browse_url(c, key).into());
    let mut out = Value::Object(out);
    strip_media(&mut out);
    out
}

/// Keep only `keys` of an object, dropping missing and null ones.
fn pick(v: &Value, keys: &[&str]) -> Option<Value> {
    let m = v.as_object()?;
    let out: Map<String, Value> = keys
        .iter()
        .filter_map(|k| {
            m.get(*k)
                .filter(|x| !x.is_null())
                .map(|x| (k.to_string(), x.clone()))
        })
        .collect();
    (!out.is_empty()).then_some(Value::Object(out))
}

/// twg's compact work item row for query and search results. Fields that
/// are empty on the issue are left out.
fn row(c: &Client, issue: &Value) -> Value {
    let f = &issue["fields"];
    let key = issue["key"].as_str().unwrap_or_default();
    let mut o = Map::new();
    o.insert("id".into(), issue["id"].clone());
    o.insert("key".into(), key.into());
    o.insert("self".into(), issue["self"].clone());
    o.insert("url".into(), browse_url(c, key).into());
    let mut put = |k: &str, v: Option<Value>| {
        if let Some(v) = v.filter(|v| !v.is_null()) {
            o.insert(k.into(), v);
        }
    };
    put("summary", Some(f["summary"].clone()));
    put(
        "status",
        pick(&f["status"], &["id", "name"]).map(|mut s| {
            if let Some(cat) = pick(&f["status"]["statusCategory"], &["key", "name"]) {
                s["statusCategory"] = cat;
            }
            s
        }),
    );
    put("issueType", pick(&f["issuetype"], &["id", "name"]));
    put("priority", pick(&f["priority"], &["id", "name"]));
    put(
        "assignee",
        pick(&f["assignee"], &["accountId", "displayName"]),
    );
    put(
        "reporter",
        pick(&f["reporter"], &["accountId", "displayName"]),
    );
    put("project", pick(&f["project"], &["id", "key", "name"]));
    put("created", Some(f["created"].clone()));
    put("updated", Some(f["updated"].clone()));
    Value::Object(o)
}

// --- reads -------------------------------------------------------------------

pub struct GetOpts {
    /// `--fields`: fetch exactly these fields instead of [`GET_FIELDS`].
    pub fields: Option<Vec<String>>,
    /// `--field`: extra fields on top of the default or `--fields` list.
    pub extra: Vec<String>,
    pub comments: bool,
    pub remote_links: bool,
    pub full: bool,
}

impl GetOpts {
    fn field_param(&self) -> String {
        if self.full {
            return "*all".into();
        }
        let mut all: Vec<String> = match &self.fields {
            Some(f) => f.clone(),
            None => GET_FIELDS.iter().map(|s| s.to_string()).collect(),
        };
        for e in &self.extra {
            if !all.contains(e) {
                all.push(e.clone());
            }
        }
        all.join(",")
    }
}

/// `jira workitem get`: the REST issue, flattened as twg prints it. With
/// `--comments` (or `--full`) the complete `comments` list is added, and
/// with `--remote-links` (or `--full`) the `remoteLinks`.
pub fn get(c: &Client, key: &str, o: &GetOpts) -> Result<Value> {
    let v = c.get(
        &format!("/rest/api/3/issue/{}", enc(key)),
        &[("fields", &o.field_param())],
    )?;
    let key = v["key"].as_str().unwrap_or(key).to_string();
    let mut out = flatten(c, &v);
    let url = out
        .as_object_mut()
        .and_then(|m| m.shift_remove("url"))
        .unwrap_or(Value::Null);
    if o.comments || o.full {
        out["comments"] = comments(c, &key)?;
    }
    if o.remote_links || o.full {
        out["remoteLinks"] = c.get(&format!("/rest/api/3/issue/{}/remotelink", enc(&key)), &[])?;
    }
    out["url"] = url;
    strip_media(&mut out);
    Ok(out)
}

/// Every comment of a work item, oldest first, as REST returns them.
fn comments(c: &Client, key: &str) -> Result<Value> {
    let mut out = Vec::new();
    loop {
        let start = out.len().to_string();
        let v = c.get(
            &format!("/rest/api/3/issue/{}/comment", enc(key)),
            &[
                ("startAt", &start),
                ("maxResults", "100"),
                ("orderBy", "created"),
            ],
        )?;
        let page = v["comments"].as_array().cloned().unwrap_or_default();
        let empty = page.is_empty();
        out.extend(page);
        if empty || out.len() as u64 >= v["total"].as_u64().unwrap_or(0) {
            return Ok(Value::Array(out));
        }
    }
}

/// Work item results: `{"issues": [...]}` and the cursor of the next page.
pub struct Found {
    pub data: Value,
    /// Jira's `nextPageToken`, which twg prints as `pageInfo.nextCursor`.
    pub next: Option<String>,
}

/// `jira workitem query`: JQL, following `nextPageToken` up to `limit`.
/// `after` resumes from a previous cursor. Without `fields` each issue is
/// twg's compact row; with `fields` it is the flattened REST issue holding
/// just those fields.
pub fn query(
    c: &Client,
    jql: &str,
    limit: usize,
    fields: Option<&[String]>,
    after: Option<&str>,
) -> Result<Found> {
    let field_param = fields.map_or_else(|| LIST_FIELDS.join(","), |f| f.join(","));
    let mut issues = Vec::new();
    let mut token = after.map(String::from);
    loop {
        let page = (limit - issues.len()).min(100).to_string();
        let mut q = vec![
            ("jql", jql),
            ("maxResults", page.as_str()),
            ("fields", field_param.as_str()),
        ];
        if let Some(t) = &token {
            q.push(("nextPageToken", t));
        }
        let v = c.get("/rest/api/3/search/jql", &q)?;
        let batch = v["issues"].as_array().cloned().unwrap_or_default();
        issues.extend(
            batch
                .iter()
                .take(limit - issues.len())
                .map(|i| issue_row(c, i, fields.is_some())),
        );
        token = v["nextPageToken"].as_str().map(String::from);
        if batch.is_empty() || token.is_none() || issues.len() >= limit {
            break;
        }
    }
    Ok(Found {
        data: json!({"issues": issues}),
        next: token,
    })
}

fn issue_row(c: &Client, issue: &Value, raw: bool) -> Value {
    if raw {
        flatten(c, issue)
    } else {
        row(c, issue)
    }
}

/// The JQL `jira workitem search` runs for `text`, as twg builds it.
pub fn search_jql(text: &str) -> String {
    format!("text ~ {} ORDER BY updated DESC", quote(text))
}

/// `jira workitem search`: fuzzy text.
pub fn search(
    c: &Client,
    text: &str,
    limit: usize,
    fields: Option<&[String]>,
    after: Option<&str>,
) -> Result<Found> {
    query(c, &search_jql(text), limit, fields, after)
}

// --- writes ------------------------------------------------------------------

fn id_or_name(s: &str, name_key: &str) -> Value {
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        json!({"id": s})
    } else {
        json!({name_key: s})
    }
}

pub struct NewWorkitem<'a> {
    pub space: &'a str,
    pub issue_type: &'a str,
    pub summary: &'a str,
    pub description: Option<(&'a str, TextFormat)>,
    pub assignee: Option<&'a str>,
    pub reporter: Option<&'a str>,
    pub priority: Option<&'a str>,
    pub labels: Vec<String>,
    pub parent: Option<&'a str>,
    pub extra: Map<String, Value>,
}

/// `jira workitem create`
pub fn create(c: &Client, n: NewWorkitem) -> Result<Value> {
    let mut fields = Map::new();
    fields.insert("project".into(), id_or_name(n.space, "key"));
    fields.insert("issuetype".into(), id_or_name(n.issue_type, "name"));
    fields.insert("summary".into(), n.summary.into());
    if let Some((d, f)) = n.description {
        fields.insert("description".into(), to_adf(d, f));
    }
    if let Some(a) = n.assignee {
        fields.insert(
            "assignee".into(),
            json!({"accountId": resolve_account(c, a)?}),
        );
    }
    if let Some(r) = n.reporter {
        fields.insert(
            "reporter".into(),
            json!({"accountId": resolve_account(c, r)?}),
        );
    }
    if let Some(p) = n.priority {
        fields.insert("priority".into(), id_or_name(p, "name"));
    }
    if !n.labels.is_empty() {
        fields.insert("labels".into(), json!(n.labels));
    }
    if let Some(p) = n.parent {
        fields.insert("parent".into(), json!({"key": p}));
    }
    fields.extend(n.extra);
    let v = c.post("/rest/api/3/issue", &json!({"fields": fields}))?;
    let key = v["key"].as_str().unwrap_or_default();
    Ok(json!({"id": v["id"], "key": key, "self": v["self"], "url": browse_url(c, key)}))
}

/// Set / add / remove a multi-value field the way twg's `--x`, `--add-x`, `--remove-x` do.
pub struct ListEdit {
    pub set: Option<Vec<String>>,
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

#[derive(Default)]
pub struct Update<'a> {
    pub summary: Option<&'a str>,
    pub description: Option<(&'a str, TextFormat)>,
    pub assignee: Option<&'a str>,
    pub issue_type: Option<&'a str>,
    pub priority: Option<&'a str>,
    pub labels: Option<ListEdit>,
    pub components: Option<ListEdit>,
    pub fix_versions: Option<ListEdit>,
    pub parent: Option<&'a str>,
    pub due_date: Option<&'a str>,
    pub resolution: Option<&'a str>,
    pub extra: Map<String, Value>,
    pub status: Option<&'a str>,
    pub transition_comment: Option<(&'a str, TextFormat)>,
    pub comment: Option<(&'a str, TextFormat)>,
}

fn none_or(s: &str) -> Option<&str> {
    (!s.eq_ignore_ascii_case("none")).then_some(s)
}

/// `jira workitem update`: field edits, then optional transition and comment.
pub fn update(c: &Client, key: &str, u: Update) -> Result<Value> {
    for (name, edit) in [
        ("labels", &u.labels),
        ("components", &u.components),
        ("fixVersions", &u.fix_versions),
    ] {
        if let Some(edit) = edit
            && edit.set.is_some()
            && (!edit.add.is_empty() || !edit.remove.is_empty())
        {
            bail!("cannot both set and add/remove '{name}' in the same update");
        }
    }
    let mut fields = Map::new();
    let mut ops = Map::new();
    let mut changed: Vec<&str> = Vec::new();
    let set = |name: &'static str, v: Value, fields: &mut Map<String, Value>| {
        fields.insert(name.into(), v);
        name
    };
    if let Some(s) = u.summary {
        changed.push(set("summary", s.into(), &mut fields));
    }
    if let Some((d, f)) = u.description {
        changed.push(set("description", to_adf(d, f), &mut fields));
    }
    if let Some(a) = u.assignee {
        let v = match none_or(a) {
            None => Value::Null,
            Some(a) => json!({"accountId": resolve_account(c, a)?}),
        };
        changed.push(set("assignee", v, &mut fields));
    }
    if let Some(t) = u.issue_type {
        changed.push(set("issuetype", id_or_name(t, "name"), &mut fields));
    }
    if let Some(p) = u.priority {
        changed.push(set("priority", id_or_name(p, "name"), &mut fields));
    }
    if let Some(p) = u.parent {
        changed.push(set(
            "parent",
            none_or(p).map_or(Value::Null, |p| json!({"key": p})),
            &mut fields,
        ));
    }
    if let Some(d) = u.due_date {
        changed.push(set(
            "duedate",
            none_or(d).map_or(Value::Null, Value::from),
            &mut fields,
        ));
    }
    if let Some(r) = u.resolution {
        changed.push(set(
            "resolution",
            none_or(r).map_or(Value::Null, |r| id_or_name(r, "name")),
            &mut fields,
        ));
    }
    for (name, edit, named) in [
        ("labels", u.labels, false),
        ("components", u.components, true),
        ("fixVersions", u.fix_versions, true),
    ] {
        let Some(edit) = edit else { continue };
        let wrap = |s: &String| if named { json!({"name": s}) } else { json!(s) };
        if let Some(all) = &edit.set {
            changed.push(set(name, all.iter().map(wrap).collect(), &mut fields));
        }
        let mut list: Vec<Value> = edit.add.iter().map(|x| json!({"add": wrap(x)})).collect();
        list.extend(edit.remove.iter().map(|x| json!({"remove": wrap(x)})));
        if !list.is_empty() {
            ops.insert(name.into(), list.into());
            changed.push(name);
        }
    }
    let extra_keys: Vec<String> = u.extra.keys().cloned().collect();
    fields.extend(u.extra);

    if fields.is_empty() && ops.is_empty() && u.status.is_none() && u.comment.is_none() {
        bail!("nothing to update; pass at least one field, --status or --comment");
    }
    let mut out = json!({"key": key, "updated": changed});
    out["updated"]
        .as_array_mut()
        .unwrap()
        .extend(extra_keys.into_iter().map(Value::from));
    if !fields.is_empty() || !ops.is_empty() {
        c.put(
            &format!("/rest/api/3/issue/{}", enc(key)),
            &json!({"fields": fields, "update": ops}),
        )?;
    }
    if let Some(s) = u.status {
        let t = transition(c, key, Some(s), Map::new(), u.transition_comment)?;
        out["status"] = t["status"].clone();
        if !t["commentId"].is_null() {
            out["transitionCommentId"] = t["commentId"].clone();
        }
    }
    if let Some((body, f)) = u.comment {
        out["commentId"] = comment(c, key, body, f, None, false, None)?["id"].clone();
    }
    out["url"] = browse_url(c, key).into();
    Ok(out)
}

/// `jira workitem comment create`; `parent` makes it a reply.
pub fn comment(
    c: &Client,
    key: &str,
    body: &str,
    f: TextFormat,
    visibility: Option<(&str, &str)>,
    internal: bool,
    parent: Option<&str>,
) -> Result<Value> {
    let mut req = json!({"body": to_adf(body, f)});
    // A reply names its parent comment; REST takes the ID as a number.
    if let Some(p) = parent {
        req["parentId"] = p
            .trim()
            .parse::<u64>()
            .map_or_else(|_| Value::from(p.trim()), Value::from);
    }
    if let Some((kind, value)) = visibility {
        req["visibility"] = json!({"type": kind, "value": value});
    }
    if internal {
        req["properties"] = json!([{"key": "sd.public.comment", "value": {"internal": true}}]);
    }
    let mut v = c.post(&format!("/rest/api/3/issue/{}/comment", enc(key)), &req)?;
    let id = v["id"].as_str().unwrap_or_default().to_string();
    if v.is_object() {
        v["issueKey"] = key.into();
        v["url"] = format!("{}?focusedCommentId={id}", browse_url(c, key)).into();
    }
    Ok(v)
}

/// The REST transitions available on a work item.
fn raw_transitions(c: &Client, key: &str) -> Result<Vec<Value>> {
    let v = c.get(&format!("/rest/api/3/issue/{}/transitions", enc(key)), &[])?;
    Ok(v["transitions"].as_array().cloned().unwrap_or_default())
}

/// `jira workitem transitions query`: `{"transitions": [{id, name, toName,
/// category}]}`, where `category` is the target status's category.
pub fn transitions(c: &Client, key: &str) -> Result<Value> {
    let list: Vec<Value> = raw_transitions(c, key)?
        .iter()
        .map(|t| {
            json!({
                "id": t["id"],
                "name": t["name"],
                "toName": t["to"]["name"],
                "category": t["to"]["statusCategory"]["name"],
            })
        })
        .collect();
    Ok(json!({"transitions": list}))
}

/// `jira workitem transition`: without a target, list transitions (read-only).
/// The target matches a transition id, transition name, or destination status.
pub fn transition(
    c: &Client,
    key: &str,
    target: Option<&str>,
    fields: Map<String, Value>,
    transition_comment: Option<(&str, TextFormat)>,
) -> Result<Value> {
    let Some(target) = target else {
        return transitions(c, key);
    };
    let list = raw_transitions(c, key)?;
    let is = |v: &Value| v.as_str().is_some_and(|s| s.eq_ignore_ascii_case(target));
    let Some(t) = list
        .iter()
        .find(|t| is(&t["id"]))
        .or_else(|| list.iter().find(|t| is(&t["name"]) || is(&t["to"]["name"])))
    else {
        let names: Vec<_> = list.iter().filter_map(|t| t["name"].as_str()).collect();
        bail!(
            "no transition '{target}' for {key}; available: {}",
            names.join(", ")
        );
    };
    let mut req = json!({"transition": {"id": t["id"]}});
    if !fields.is_empty() {
        req["fields"] = fields.into();
    }
    // Jira silently drops a transition comment when the transition has no
    // screen, so in that case add it as a regular comment afterwards.
    let has_screen = t["hasScreen"] == true;
    if let Some((body, f)) = transition_comment.filter(|_| has_screen) {
        req["update"] = json!({"comment": [{"add": {"body": to_adf(body, f)}}]});
    }
    c.post(&format!("/rest/api/3/issue/{}/transitions", enc(key)), &req)?;
    let mut out = json!({
        "key": key,
        "transition": {"id": t["id"], "name": t["name"]},
        "status": t["to"]["name"],
        "url": browse_url(c, key),
    });
    if let Some((body, f)) = transition_comment.filter(|_| !has_screen) {
        out["commentId"] = comment(c, key, body, f, None, false, None)?["id"].clone();
    }
    Ok(out)
}

pub fn myself(c: &Client) -> Result<Value> {
    c.get("/rest/api/3/myself", &[])
}

/// twg's user profile shape (`whoami`) from Jira's `myself`. Fields Jira
/// REST doesn't expose (`extendedProfile`) are null.
pub fn profile(me: &Value) -> Value {
    json!({
        "accountId": me["accountId"],
        "name": me["displayName"],
        "picture": me["avatarUrls"]["48x48"],
        "accountStatus": match me["active"].as_bool() {
            Some(true) => "active",
            Some(false) => "inactive",
            None => "unknown",
        },
        "email": me["emailAddress"],
        "nickname": me["displayName"],
        "locale": me["locale"],
        "zoneinfo": me["timeZone"],
        "extendedProfile": {
            "jobTitle": null,
            "department": null,
            "organization": null,
            "location": null,
        },
    })
}

fn fetch_users(c: &Client, query: &str, max_results: Option<usize>) -> Result<Vec<Value>> {
    let max = max_results.map(|n| n.clamp(1, 1000).to_string());
    let mut q = vec![("query", query)];
    if let Some(m) = &max {
        q.push(("maxResults", m.as_str()));
    }
    let v = c.get("/rest/api/3/user/search", &q)?;
    Ok(v.as_array().cloned().unwrap_or_default())
}

/// One `user search` result in twg's shape. `sources` names where the
/// match came from; atlx only searches Jira.
fn user_row(u: &Value) -> Value {
    let id = u["accountId"].as_str().unwrap_or_default();
    let mut o = json!({
        "accountId": id,
        "userAri": format!("ari:cloud:identity::user/{id}"),
        "name": u["displayName"],
    });
    if let Some(email) = u["emailAddress"].as_str().filter(|s| !s.is_empty()) {
        o["email"] = email.into();
    }
    o["sources"] = json!(["jira-rest"]);
    o
}

/// `user search`
pub fn user_search(c: &Client, query: &str, limit: usize) -> Result<Value> {
    if limit == 0 {
        return Ok(json!([]));
    }
    let users = fetch_users(c, query, Some(limit))?;
    Ok(users.iter().take(limit).map(user_row).collect())
}

/// `user search --account-id`: one user by account ID (or identity ARI), as
/// a one-element list so the output matches a search.
pub fn user_by_account(c: &Client, account: &str) -> Result<Value> {
    let id = account
        .strip_prefix("ari:cloud:identity::user/")
        .unwrap_or(account);
    let v = c.get("/rest/api/3/user", &[("accountId", id)])?;
    Ok(json!([user_row(&v)]))
}

/// `me`, a raw accountId, or a name/email that matches exactly one user.
fn resolve_account(c: &Client, who: &str) -> Result<String> {
    if who.eq_ignore_ascii_case("me") {
        return myself(c)?["accountId"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow!("no accountId"));
    }
    if let Some(id) = who.strip_prefix("ari:cloud:identity::user/") {
        return Ok(id.to_string());
    }
    if who.contains(':') || (who.len() >= 24 && who.chars().all(|c| c.is_ascii_alphanumeric())) {
        return Ok(who.to_string());
    }
    let users = fetch_users(c, who, None)?;
    let users: Vec<&Value> = users
        .iter()
        .filter(|u| u["accountType"] == "atlassian")
        .collect();
    match users.as_slice() {
        [u] => Ok(u["accountId"].as_str().unwrap_or_default().to_string()),
        [] => bail!("no user matches '{who}'"),
        many => {
            let names: Vec<_> = many
                .iter()
                .filter_map(|u| u["displayName"].as_str())
                .collect();
            bail!(
                "'{who}' matches several users: {}; pass an account ID",
                names.join(", ")
            )
        }
    }
}

// --- metadata: fields, projects, create-meta ---------------------------------

pub fn filter_fields(fields: &[Value], query: Option<&str>, custom_only: bool) -> Vec<Value> {
    let q = query
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    fields
        .iter()
        .filter(|f| !custom_only || f["custom"].as_bool() == Some(true))
        .filter(|f| {
            let Some(q) = &q else { return true };
            let name = f["name"].as_str().unwrap_or_default().to_lowercase();
            let id = f["id"].as_str().unwrap_or_default().to_lowercase();
            name.contains(q) || id.contains(q)
        })
        .map(|f| {
            json!({
                "id": f["id"],
                "name": f["name"],
                "custom": f["custom"],
                "type": f["schema"]["type"],
            })
        })
        .collect()
}

/// `jira field list`
pub fn fields(c: &Client, query: Option<&str>, custom_only: bool) -> Result<Value> {
    let v = c.get("/rest/api/3/field", &[])?;
    let list = v.as_array().map(Vec::as_slice).unwrap_or(&[]);
    Ok(Value::Array(filter_fields(list, query, custom_only)))
}

/// Items collected from an offset-paged endpoint.
pub struct Paged {
    pub values: Vec<Value>,
    pub start: usize,
    pub limit: usize,
    /// `total` from the last page, when the endpoint reports it.
    pub total: Option<u64>,
    /// Whether the last item of the collection was reached.
    pub is_last: bool,
    /// The first page's `self` link, when the endpoint gives one.
    pub self_link: Option<Value>,
}

impl Paged {
    /// The REST page envelope twg prints, with the list under `collection`
    /// (`projects`, `boards`, `sprints`): `{collection, total, startAt,
    /// maxResults, isLast}`.
    pub fn to_value(&self, collection: &str) -> Value {
        let mut m = Map::new();
        m.insert(collection.into(), Value::Array(self.values.clone()));
        if let Some(t) = self.total {
            m.insert("total".into(), t.into());
        }
        m.insert("startAt".into(), self.start.into());
        m.insert("maxResults".into(), self.limit.into());
        m.insert("isLast".into(), self.is_last.into());
        if let Some(s) = &self.self_link {
            m.insert("self".into(), s.clone());
        }
        let mut v = Value::Object(m);
        strip_media(&mut v);
        v
    }
}

/// Collect up to `limit` items from an offset-paged endpoint, starting at
/// item `offset`.
fn paginate_values(
    c: &Client,
    path: &str,
    query: &[(&str, &str)],
    offset: usize,
    limit: usize,
) -> Result<Paged> {
    let mut out = Paged {
        values: Vec::new(),
        start: offset,
        limit,
        total: None,
        is_last: true,
        self_link: None,
    };
    if limit == 0 {
        return Ok(out);
    }
    loop {
        let start = (offset + out.values.len()).to_string();
        let max_page = (limit - out.values.len()).min(50);
        let page_size = max_page.to_string();
        let mut q = query.to_vec();
        q.push(("startAt", &start));
        q.push(("maxResults", &page_size));
        let v = c.get(path, &q)?;
        if out.self_link.is_none() {
            out.self_link = v.get("self").cloned();
        }
        let page = v["values"].as_array().cloned().unwrap_or_default();
        let page_len = page.len();
        out.values.extend(page);
        let is_last = v["isLast"].as_bool();
        let total = v["total"].as_u64();
        out.total = total;
        let seen = (offset + out.values.len()) as u64;
        let done = page_len == 0
            || is_last == Some(true)
            || total.is_some_and(|t| seen >= t)
            || (is_last.is_none() && total.is_none() && page_len < max_page);
        if done || out.values.len() >= limit {
            out.is_last = done && out.values.len() <= limit;
            break;
        }
    }
    out.values.truncate(limit);
    Ok(out)
}

/// Filters for `jira space query` (Jira REST `project/search`).
#[derive(Default)]
pub struct SpaceQuery<'a> {
    pub query: Option<&'a str>,
    /// Project keys; repeatable.
    pub keys: &'a [String],
    /// Project IDs; repeatable.
    pub ids: &'a [String],
    pub type_key: Option<&'a str>,
    pub category_id: Option<&'a str>,
    pub action: Option<&'a str>,
    pub expand: Option<&'a str>,
    pub status: Option<&'a str>,
    pub properties: &'a [String],
    pub property_query: Option<&'a str>,
    /// Defaults to `key`, as in twg.
    pub order_by: Option<&'a str>,
    pub start: usize,
    pub limit: usize,
}

/// `jira space query` (and the old `jira project list`): REST
/// `project/search` with its `values` renamed `projects`, as twg prints it.
pub fn space_query(c: &Client, o: &SpaceQuery) -> Result<Value> {
    let mut q = Vec::new();
    if let Some(s) = o.query.map(str::trim).filter(|s| !s.is_empty()) {
        q.push(("query", s));
    }
    q.extend(o.keys.iter().map(|k| ("keys", k.as_str())));
    q.extend(o.ids.iter().map(|i| ("id", i.as_str())));
    q.extend(o.properties.iter().map(|p| ("properties", p.as_str())));
    for (k, v) in [
        ("typeKey", o.type_key),
        ("categoryId", o.category_id),
        ("action", o.action),
        ("expand", o.expand),
        ("status", o.status),
        ("propertyQuery", o.property_query),
        ("orderBy", Some(o.order_by.unwrap_or("key"))),
    ] {
        if let Some(v) = v {
            q.push((k, v));
        }
    }
    let page = paginate_values(c, "/rest/api/3/project/search", &q, o.start, o.limit)?;
    Ok(page.to_value("projects"))
}

/// `jira space get` (and the old `jira project get`): a one-element list
/// with the project's main fields, lead and issue types, as twg prints it.
pub fn project(c: &Client, key_or_id: &str) -> Result<Value> {
    let v = c.get(&format!("/rest/api/3/project/{}", enc(key_or_id)), &[])?;
    let mut out = Map::new();
    for k in ["id", "key", "name", "projectTypeKey", "description"] {
        out.insert(k.into(), v[k].clone());
    }
    out.insert(
        "lead".into(),
        pick(&v["lead"], &["self", "accountId", "displayName", "active"]).unwrap_or(Value::Null),
    );
    let types: Vec<Value> = v["issueTypes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(issue_type_row)
        .collect();
    out.insert("issueTypes".into(), types.into());
    Ok(json!([out]))
}

/// A REST issue type without its avatar URL and entity ID.
fn issue_type_row(t: &Value) -> Value {
    pick(
        t,
        &[
            "self",
            "id",
            "description",
            "name",
            "subtask",
            "avatarId",
            "hierarchyLevel",
        ],
    )
    .unwrap_or(Value::Null)
}

fn paginate_createmeta(c: &Client, path: &str, key: &str, alt_key: &str) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    loop {
        let start = out.len().to_string();
        let v = c.get(path, &[("startAt", &start), ("maxResults", "50")])?;
        let page = v[key]
            .as_array()
            .or_else(|| v[alt_key].as_array())
            .cloned()
            .unwrap_or_default();
        let page_len = page.len();
        out.extend(page);
        let total = v["total"].as_u64();
        let is_last = v["isLast"].as_bool();
        if page_len == 0
            || is_last == Some(true)
            || total.is_some_and(|t| out.len() as u64 >= t)
            || (is_last.is_none() && total.is_none() && page_len < 50)
        {
            return Ok(out);
        }
    }
}

fn resolve_issue_type<'a>(types: &'a [Value], target: &str) -> Result<&'a Value> {
    let is = |t: &Value, k: &str| {
        t[k].as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(target))
    };
    types
        .iter()
        .find(|t| is(t, "id"))
        .or_else(|| types.iter().find(|t| is(t, "name")))
        .ok_or_else(|| {
            let names: Vec<_> = types.iter().filter_map(|t| t["name"].as_str()).collect();
            anyhow!("no issue type '{target}'; available: {}", names.join(", "))
        })
}

/// What a `--field NAME=VALUE` value means for a field schema, in twg's
/// words (`date`, `multi-option`, ...).
fn accepted_input(schema: &Value) -> &'static str {
    let ty = schema["type"].as_str().unwrap_or_default();
    let items = schema["items"].as_str().unwrap_or_default();
    match (ty, items) {
        ("array", "option") => "multi-option",
        ("array", "user") => "multi-user",
        ("array", "version") => "multi-version",
        ("array", "component") => "multi-component",
        ("array", "string") => "multi-text",
        ("option", _) | ("option-with-child", _) => "option",
        ("date", _) => "date",
        ("datetime", _) => "datetime",
        ("number", _) => "number",
        ("string", _) => "text",
        ("user", _) => "user",
        ("version", _) => "version",
        _ => "value",
    }
}

/// The custom fields of a create screen, in twg's shape: REST's field
/// metadata plus `acceptedInput` and an `example` `--field` argument.
/// Standard fields have their own flags, so they are left out, as in twg.
fn create_fields(fields: &[Value]) -> Vec<Value> {
    let mut custom: Vec<&Value> = fields
        .iter()
        .filter(|f| {
            f["fieldId"]
                .as_str()
                .or_else(|| f["key"].as_str())
                .is_some_and(|id| id.starts_with("customfield_"))
        })
        .collect();
    custom.sort_by_key(|f| {
        let id = f["fieldId"].as_str().or(f["key"].as_str()).unwrap_or("");
        let n: u64 = id
            .trim_start_matches("customfield_")
            .parse()
            .unwrap_or(u64::MAX);
        n
    });
    custom
        .into_iter()
        .map(|f| {
            let id = f["fieldId"].as_str().or(f["key"].as_str()).unwrap_or("");
            let name = f["name"].as_str().unwrap_or(id);
            let first = f["allowedValues"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|a| a["value"].as_str().or(a["name"].as_str()))
                .unwrap_or("VALUE");
            let mut o = Map::new();
            o.insert("id".into(), id.into());
            o.insert("key".into(), f["key"].as_str().unwrap_or(id).into());
            o.insert("name".into(), name.into());
            o.insert("required".into(), f["required"].clone());
            o.insert("schema".into(), f["schema"].clone());
            for k in ["allowedValues", "defaultValue"] {
                if let Some(v) = f.get(k) {
                    o.insert(k.into(), v.clone());
                }
            }
            o.insert("operations".into(), f["operations"].clone());
            o.insert("acceptedInput".into(), accepted_input(&f["schema"]).into());
            o.insert(
                "example".into(),
                format!("--field \"{name}={first}\"").into(),
            );
            Value::Object(o)
        })
        .collect()
}

/// `jira workitem field create-metadata`: with an issue type,
/// `{"fields": [...]}` (custom fields, as twg prints them); without one,
/// `{"issueTypes": [...]}` for the space.
pub fn create_meta(c: &Client, space: &str, issue_type: Option<&str>) -> Result<Value> {
    let types_path = format!("/rest/api/3/issue/createmeta/{}/issuetypes", enc(space));
    let types = paginate_createmeta(c, &types_path, "issueTypes", "values")?;
    let Some(target) = issue_type else {
        return Ok(json!({
            "issueTypes": types.iter().map(issue_type_row).collect::<Vec<_>>(),
        }));
    };
    let it = resolve_issue_type(&types, target)?;
    let it_id = it["id"].as_str().unwrap_or_default();
    let fields_path = format!("{types_path}/{}", enc(it_id));
    let raw_fields = paginate_createmeta(c, &fields_path, "fields", "values")?;
    let mut out = json!({"fields": create_fields(&raw_fields)});
    strip_media(&mut out);
    Ok(out)
}

// --- issue & remote links ----------------------------------------------------

/// `jira workitem link-types query`: REST's link types as a list.
pub fn link_types(c: &Client) -> Result<Value> {
    let v = c.get("/rest/api/3/issueLinkType", &[])?;
    let mut types = v["issueLinkTypes"].clone();
    strip_media(&mut types);
    Ok(types)
}

fn build_issue_link_payload(
    inward: &str,
    outward: &str,
    link_type: &str,
    comment: Option<(&str, TextFormat)>,
) -> Value {
    let mut req = json!({
        "type": id_or_name(link_type, "name"),
        "inwardIssue": id_or_name(inward, "key"),
        "outwardIssue": id_or_name(outward, "key"),
    });
    if let Some((body, fmt)) = comment {
        req["comment"] = json!({"body": to_adf(body, fmt)});
    }
    req
}

/// `jira workitem link create`
pub fn link_create(
    c: &Client,
    inward: &str,
    outward: &str,
    link_type: &str,
    comment: Option<(&str, TextFormat)>,
) -> Result<Value> {
    let req = build_issue_link_payload(inward, outward, link_type, comment);
    c.post("/rest/api/3/issueLink", &req)?;
    Ok(json!({
        "inward": inward,
        "outward": outward,
        "type": link_type,
        "inwardUrl": browse_url(c, inward),
        "outwardUrl": browse_url(c, outward),
    }))
}

/// `jira workitem link workitem`: link `source` to each target.
///
/// The source reads with the link type's outward phrase, so
/// `--id PROJ-1 --target-id PROJ-2 --link-type-id Blocks` records "PROJ-1
/// blocks PROJ-2". Jira's REST API names that side `inwardIssue`, so the
/// source is sent as `inwardIssue` and the target as `outwardIssue`. One
/// target returns that link; several return an array in target order.
pub fn link_workitem(
    c: &Client,
    source: &str,
    targets: &[String],
    link_type: &str,
) -> Result<Value> {
    if targets.is_empty() {
        bail!("pass at least one --target-id");
    }
    let mut links = Vec::with_capacity(targets.len());
    for t in targets {
        links.push(link_create(c, source, t, link_type, None)?);
    }
    Ok(match links.len() {
        1 => links.remove(0),
        _ => Value::Array(links),
    })
}

/// A remote web link (`jira workitem link weblink`).
#[derive(Default)]
pub struct Weblink<'a> {
    pub url: &'a str,
    pub title: &'a str,
    pub relationship: Option<&'a str>,
    pub global_id: Option<&'a str>,
    pub application_name: Option<&'a str>,
    pub application_type: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub icon_url: Option<&'a str>,
    pub resolved: bool,
}

fn build_remote_link_payload(w: &Weblink) -> Value {
    let mut req = json!({
        "object": {
            "url": w.url,
            "title": w.title,
        }
    });
    if let Some(rel) = w.relationship {
        req["relationship"] = rel.into();
    }
    if let Some(g) = w.global_id {
        req["globalId"] = g.into();
    }
    if w.application_name.is_some() || w.application_type.is_some() {
        let mut app = json!({});
        if let Some(n) = w.application_name {
            app["name"] = n.into();
        }
        if let Some(t) = w.application_type {
            app["type"] = t.into();
        }
        req["application"] = app;
    }
    if let Some(s) = w.summary {
        req["object"]["summary"] = s.into();
    }
    if let Some(i) = w.icon_url {
        req["object"]["icon"] = json!({"url16x16": i});
    }
    if w.resolved {
        req["object"]["status"] = json!({"resolved": true});
    }
    req
}

/// `jira workitem link weblink` (and the old `link remote`)
pub fn link_remote(c: &Client, key: &str, w: &Weblink) -> Result<Value> {
    let req = build_remote_link_payload(w);
    let v = c.post(&format!("/rest/api/3/issue/{}/remotelink", enc(key)), &req)?;
    let mut out = json!({
        "id": v["id"],
        "self": v["self"],
        "key": key,
        "title": w.title,
        "url": w.url,
        "issueUrl": browse_url(c, key),
    });
    if let Some(rel) = w.relationship {
        out["relationship"] = rel.into();
    }
    Ok(out)
}

/// `jira workitem link query`: the work item's issue links as REST lists
/// them in its `issuelinks` field.
pub fn link_query(c: &Client, key: &str) -> Result<Value> {
    let v = c.get(
        &format!("/rest/api/3/issue/{}", enc(key)),
        &[("fields", "issuelinks")],
    )?;
    let mut links = match &v["fields"]["issuelinks"] {
        Value::Array(a) => Value::Array(a.clone()),
        _ => json!([]),
    };
    strip_media(&mut links);
    Ok(links)
}

/// `jira workitem unlink workitem`
pub fn unlink_workitem(c: &Client, link_id: &str) -> Result<Value> {
    c.delete(&format!("/rest/api/3/issueLink/{}", enc(link_id)), &[])?;
    Ok(json!({"linkId": link_id, "deleted": true}))
}

/// Which remote link `jira workitem unlink weblink` removes.
#[derive(Clone, Copy, Debug)]
pub enum WeblinkRef<'a> {
    Id(&'a str),
    GlobalId(&'a str),
    /// The link whose `object.url` is this URL.
    Url(&'a str),
}

/// `jira workitem unlink weblink`: by link ID, by global ID, or by finding
/// the one remote link that points at a URL.
pub fn unlink_weblink(c: &Client, key: &str, r: WeblinkRef) -> Result<Value> {
    let base = format!("/rest/api/3/issue/{}/remotelink", enc(key));
    let mut out = json!({"id": key});
    match r {
        WeblinkRef::Id(id) => {
            c.delete(&format!("{base}/{}", enc(id)), &[])?;
            out["linkId"] = id.into();
        }
        WeblinkRef::GlobalId(g) => {
            c.delete(&base, &[("globalId", g)])?;
            out["globalId"] = g.into();
        }
        WeblinkRef::Url(url) => {
            let links = c.get(&base, &[])?;
            let want = url.trim_end_matches('/');
            let hits: Vec<&Value> = links
                .as_array()
                .into_iter()
                .flatten()
                .filter(|l| {
                    l["object"]["url"]
                        .as_str()
                        .is_some_and(|u| u.trim_end_matches('/') == want)
                })
                .collect();
            let id = match hits.as_slice() {
                [l] => match &l["id"] {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.clone(),
                    _ => bail!("the remote link for {url} has no ID"),
                },
                [] => {
                    return Err(ApiError::new(
                        ErrorKind::NotFound,
                        format!("{key} has no remote link to {url}"),
                    )
                    .into());
                }
                many => {
                    let ids: Vec<String> = many.iter().map(|l| l["id"].to_string()).collect();
                    return Err(ApiError::usage(format!(
                        "{key} has several remote links to {url} (IDs {}); pass --link-id",
                        ids.join(", ")
                    ))
                    .into());
                }
            };
            c.delete(&format!("{base}/{}", enc(&id)), &[])?;
            out["linkId"] = id.into();
            out["url"] = url.into();
        }
    }
    out["deleted"] = true.into();
    Ok(out)
}

// --- comments, changelog, delete ---------------------------------------------

/// One offset page of a REST list, with what twg prints around it.
pub struct OffsetPage {
    pub data: Value,
    pub start: usize,
    pub max: usize,
    pub total: Option<u64>,
    /// The `startAt` of the next page, as a cursor string.
    pub next: Option<String>,
}

impl OffsetPage {
    /// twg's `pageInfo` for this page; it is printed on the last page too.
    pub fn page_info(&self) -> Value {
        json!({"hasNextPage": self.next.is_some(), "nextCursor": self.next})
    }

    /// twg's `meta.pagination`.
    pub fn pagination(&self) -> Value {
        json!({"startAt": self.start, "maxResults": self.max, "total": self.total})
    }
}

/// A cursor from a previous page (`startAt` as a string) as an offset.
pub fn offset_cursor(after: Option<&str>) -> Result<usize> {
    match after.map(str::trim) {
        None | Some("") => Ok(0),
        Some(a) => a.parse().map_err(|_| {
            ApiError::usage(format!(
                "--after '{a}' is not a cursor from a previous page (a number)"
            ))
            .into()
        }),
    }
}

fn offset_page(
    c: &Client,
    path: &str,
    list_key: &str,
    extra: &[(&str, &str)],
    start: usize,
    max: usize,
) -> Result<OffsetPage> {
    let (s, m) = (start.to_string(), max.to_string());
    let mut q = vec![("startAt", s.as_str()), ("maxResults", m.as_str())];
    q.extend_from_slice(extra);
    let v = c.get(path, &q)?;
    let mut list = match &v[list_key] {
        Value::Array(a) => Value::Array(a.clone()),
        _ => json!([]),
    };
    strip_media(&mut list);
    let len = list.as_array().map_or(0, Vec::len);
    let total = v["total"].as_u64();
    let end = start + len;
    let more = match (total, v["isLast"].as_bool()) {
        (_, Some(last)) => !last && len > 0,
        (Some(t), None) => (end as u64) < t,
        (None, None) => len > 0 && len >= max,
    };
    Ok(OffsetPage {
        data: list,
        start: v["startAt"].as_u64().map_or(start, |n| n as usize),
        max: v["maxResults"].as_u64().map_or(max, |n| n as usize),
        total,
        next: more.then(|| end.to_string()),
    })
}

/// `jira workitem comment query`: one page of REST comments. `order_by` is
/// Jira's `orderBy` (`created`, `-created`, `+created`).
pub fn comment_query(
    c: &Client,
    key: &str,
    start: usize,
    max: usize,
    order_by: Option<&str>,
) -> Result<OffsetPage> {
    let mut q = Vec::new();
    if let Some(o) = order_by {
        q.push(("orderBy", o));
    }
    offset_page(
        c,
        &format!("/rest/api/3/issue/{}/comment", enc(key)),
        "comments",
        &q,
        start,
        max,
    )
}

/// `jira workitem changelog query`: one page of REST changelog entries.
pub fn changelog_query(c: &Client, key: &str, start: usize, max: usize) -> Result<OffsetPage> {
    offset_page(
        c,
        &format!("/rest/api/3/issue/{}/changelog", enc(key)),
        "values",
        &[],
        start,
        max,
    )
}

/// `jira workitem comment update`: the updated REST comment, with the same
/// `issueKey` and `url` additions as `comment create`.
pub fn comment_update(c: &Client, key: &str, id: &str, body: &str, f: TextFormat) -> Result<Value> {
    let mut v = c.put(
        &format!("/rest/api/3/issue/{}/comment/{}", enc(key), enc(id)),
        &json!({"body": to_adf(body, f)}),
    )?;
    if v.is_object() {
        v["issueKey"] = key.into();
        v["url"] = format!("{}?focusedCommentId={id}", browse_url(c, key)).into();
    }
    strip_media(&mut v);
    Ok(v)
}

/// `jira workitem comment delete`
pub fn comment_delete(c: &Client, key: &str, id: &str) -> Result<Value> {
    c.delete(
        &format!("/rest/api/3/issue/{}/comment/{}", enc(key), enc(id)),
        &[],
    )?;
    Ok(json!({"id": id, "issueId": key, "deleted": true}))
}

/// `jira workitem delete`
pub fn delete(c: &Client, key: &str, delete_subtasks: bool) -> Result<Value> {
    c.delete(
        &format!("/rest/api/3/issue/{}", enc(key)),
        &[(
            "deleteSubtasks",
            if delete_subtasks { "true" } else { "false" },
        )],
    )?;
    Ok(json!({"id": key, "deleted": true, "deleteSubtasks": delete_subtasks}))
}

// --- edit metadata, types, statuses, priorities ------------------------------

/// `acceptedInput` on the edit screen. Built-in text and label fields take
/// a plain value there, and priorities read as options.
fn edit_accepted_input(schema: &Value) -> &'static str {
    if schema["type"] == "priority" {
        return "option";
    }
    match accepted_input(schema) {
        "text" | "multi-text" if schema.get("system").is_some() => "value",
        other => other,
    }
}

/// The fields of an edit screen in twg's shape, in REST order: custom
/// fields only unless `include_system`. Fields with no operations can't be
/// edited and are left out; `example` is only given for settable fields.
pub fn edit_fields(fields: &Map<String, Value>, include_system: bool) -> Vec<Value> {
    fields
        .iter()
        .filter(|(id, _)| include_system || id.starts_with("customfield_"))
        .filter(|(_, f)| f["operations"].as_array().is_some_and(|o| !o.is_empty()))
        .map(|(id, f)| {
            let name = f["name"].as_str().unwrap_or(id);
            let mut o = Map::new();
            o.insert("id".into(), id.as_str().into());
            o.insert("key".into(), f["key"].as_str().unwrap_or(id).into());
            o.insert("name".into(), name.into());
            o.insert("required".into(), f["required"].clone());
            o.insert("schema".into(), f["schema"].clone());
            for k in ["allowedValues", "defaultValue"] {
                if let Some(v) = f.get(k) {
                    o.insert(k.into(), v.clone());
                }
            }
            o.insert("operations".into(), f["operations"].clone());
            o.insert(
                "acceptedInput".into(),
                edit_accepted_input(&f["schema"]).into(),
            );
            let settable = f["operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|x| x == "set"));
            if settable {
                let first = f["allowedValues"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|a| a["value"].as_str().or(a["name"].as_str()))
                    .unwrap_or("VALUE");
                o.insert(
                    "example".into(),
                    format!("--field \"{name}={first}\"").into(),
                );
            }
            Value::Object(o)
        })
        .collect()
}

/// `jira workitem field update-metadata`: `{"fields": [...]}` from REST
/// `editmeta`.
pub fn update_meta(c: &Client, key: &str, include_system: bool) -> Result<Value> {
    let v = c.get(&format!("/rest/api/3/issue/{}/editmeta", enc(key)), &[])?;
    let empty = Map::new();
    let fields = v["fields"].as_object().unwrap_or(&empty);
    let mut out = json!({"fields": edit_fields(fields, include_system)});
    strip_media(&mut out);
    Ok(out)
}

/// A project's key and numeric ID, from its key or ID.
pub fn project_key_id(c: &Client, key_or_id: &str) -> Result<(String, String)> {
    let v = c.get(&format!("/rest/api/3/project/{}", enc(key_or_id)), &[])?;
    let id = match &v["id"] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => bail!("project {key_or_id} has no ID"),
    };
    let key = v["key"].as_str().unwrap_or(key_or_id).to_string();
    Ok((key, id))
}

/// `jira workitem types query`: the issue types of a project (REST
/// `issuetype/project`), optionally at one hierarchy level, up to `limit`,
/// and how many there were before the limit.
pub fn issue_types(
    c: &Client,
    project_id: &str,
    level: Option<i64>,
    limit: usize,
) -> Result<(Value, usize)> {
    let level = level.map(|l| l.to_string());
    let mut q = vec![("projectId", project_id)];
    if let Some(l) = &level {
        q.push(("level", l));
    }
    let v = c.get("/rest/api/3/issuetype/project", &q)?;
    let mut list: Vec<Value> = v.as_array().cloned().unwrap_or_default();
    let found = list.len();
    list.truncate(limit);
    let mut out = Value::Array(list);
    strip_media(&mut out);
    Ok((out, found))
}

/// GET `{path}/{id}` for each ID, as a list.
fn each(c: &Client, path: &str, ids: &[String]) -> Result<Value> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        out.push(c.get(&format!("{path}/{}", enc(id)), &[])?);
    }
    let mut out = Value::Array(out);
    strip_media(&mut out);
    Ok(out)
}

/// The numeric part of an ID or ARI (`ari:...:issue-type/10001`).
pub fn entity_id(s: &str) -> String {
    let s = s.trim();
    if s.starts_with("ari:") {
        return trailing_number(s).unwrap_or_else(|| s.to_string());
    }
    s.to_string()
}

/// `jira workitem types get`
pub fn issue_types_get(c: &Client, ids: &[String]) -> Result<Value> {
    each(c, "/rest/api/3/issuetype", ids)
}

/// `jira workitem statuses query`: every status, or the given ones.
pub fn statuses(c: &Client, ids: &[String]) -> Result<Value> {
    if !ids.is_empty() {
        return each(c, "/rest/api/3/status", ids);
    }
    let mut v = c.get("/rest/api/3/status", &[])?;
    strip_media(&mut v);
    Ok(v)
}

/// `jira workitem priorities query`: every priority, or the given ones.
pub fn priorities(c: &Client, ids: &[String]) -> Result<Value> {
    if !ids.is_empty() {
        return each(c, "/rest/api/3/priority", ids);
    }
    let mut v = c.get("/rest/api/3/priority", &[])?;
    strip_media(&mut v);
    Ok(v)
}

// --- agile: boards & sprints -------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BoardType {
    Scrum,
    Kanban,
}

impl BoardType {
    fn as_str(self) -> &'static str {
        match self {
            BoardType::Scrum => "scrum",
            BoardType::Kanban => "kanban",
        }
    }
}

/// Filters for `jira board query` (Agile REST `board`).
#[derive(Default)]
pub struct BoardQuery<'a> {
    /// Project key or ID.
    pub project: Option<&'a str>,
    pub board_type: Option<BoardType>,
    /// Board name substring.
    pub name: Option<&'a str>,
    pub filter_id: Option<&'a str>,
    pub start: usize,
    pub limit: usize,
}

fn board_values(c: &Client, o: &BoardQuery) -> Result<Paged> {
    let mut q = Vec::new();
    if let Some(p) = o.project {
        q.push(("projectKeyOrId", p));
    }
    if let Some(t) = o.board_type {
        q.push(("type", t.as_str()));
    }
    if let Some(n) = o.name {
        q.push(("name", n));
    }
    if let Some(f) = o.filter_id {
        q.push(("filterId", f));
    }
    paginate_values(c, "/rest/agile/1.0/board", &q, o.start, o.limit)
}

/// `jira board query` (and the old `jira board list`): Agile REST's board
/// page with `values` renamed `boards`, as twg prints it.
pub fn board_query(c: &Client, o: &BoardQuery) -> Result<Value> {
    Ok(board_values(c, o)?.to_value("boards"))
}

/// The project key of a work item, for `--issue` filters.
pub fn issue_project(c: &Client, issue: &str) -> Result<String> {
    let v = c.get(
        &format!("/rest/api/3/issue/{}", enc(issue)),
        &[("fields", "project")],
    )?;
    v["fields"]["project"]["key"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow!("work item {issue} has no project"))
}

/// A Jira filter ID from an ID, a `filter=N` URL or a filter ARI.
pub fn filter_ref(s: &str) -> String {
    let s = s.trim();
    if let Some(i) = s.find("filter=") {
        let id: String = s[i + 7..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if !id.is_empty() {
            return id;
        }
    }
    trailing_number(s).unwrap_or_else(|| s.to_string())
}

/// The last run of digits after a `/` (ARIs, `/boards/12` URLs).
fn trailing_number(s: &str) -> Option<String> {
    s.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
        .map(String::from)
}

/// A board ID from an ID, an ARI, a board URL (`/boards/12`,
/// `rapidView=12`), or an exact board name.
pub fn resolve_board(c: &Client, r: &str) -> Result<u64> {
    let r = r.trim();
    if let Ok(n) = r.parse() {
        return Ok(n);
    }
    for marker in ["rapidView=", "/boards/"] {
        if let Some(i) = r.find(marker) {
            let id: String = r[i + marker.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(n) = id.parse() {
                return Ok(n);
            }
        }
    }
    if r.starts_with("ari:")
        && let Some(n) = trailing_number(r).and_then(|n| n.parse().ok())
    {
        return Ok(n);
    }
    let found = board_values(
        c,
        &BoardQuery {
            name: Some(r),
            limit: 50,
            ..Default::default()
        },
    )?
    .values;
    let exact: Vec<&Value> = found
        .iter()
        .filter(|b| {
            b["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(r))
        })
        .collect();
    match exact.as_slice() {
        [b] => b["id"]
            .as_u64()
            .ok_or_else(|| anyhow!("board '{r}' has no ID")),
        [] => bail!("no board named '{r}'"),
        _ => bail!("several boards are named '{r}'; pass the board ID"),
    }
}

/// The only board of a project, for sprint queries scoped by project.
pub fn project_board(c: &Client, project: &str) -> Result<u64> {
    let found = board_values(
        c,
        &BoardQuery {
            project: Some(project),
            limit: 50,
            ..Default::default()
        },
    )?
    .values;
    match found.as_slice() {
        [b] => b["id"].as_u64().ok_or_else(|| anyhow!("board has no ID")),
        [] => bail!("project {project} has no boards"),
        many => {
            let list: Vec<String> = many
                .iter()
                .map(|b| format!("{} ({})", b["id"], b["name"].as_str().unwrap_or_default()))
                .collect();
            bail!(
                "project {project} has several boards: {}; pass --id",
                list.join(", ")
            )
        }
    }
}

pub fn normalize_sprint_states(s: &str) -> Result<String> {
    let states = csv(s);
    if states.is_empty() {
        bail!("--state cannot be empty; expected active, future, or closed");
    }
    let mut out = Vec::with_capacity(states.len());
    for st in states {
        let lower = st.to_lowercase();
        match lower.as_str() {
            "active" | "future" | "closed" => out.push(lower),
            _ => bail!("invalid sprint state '{st}'; expected active, future, or closed"),
        }
    }
    Ok(out.join(","))
}

/// `jira board sprints query` (and the old `jira sprint list`); sprints
/// from item `start`, as Agile REST's page with `values` renamed `sprints`.
pub fn sprints(
    c: &Client,
    board_id: u64,
    state: Option<&str>,
    start: usize,
    limit: usize,
) -> Result<Value> {
    let state_param = state.map(normalize_sprint_states).transpose()?;
    let mut q = Vec::new();
    if let Some(st) = &state_param {
        q.push(("state", st.as_str()));
    }
    let page = paginate_values(
        c,
        &format!("/rest/agile/1.0/board/{board_id}/sprint"),
        &q,
        start,
        limit,
    )?;
    Ok(page.to_value("sprints"))
}

/// Options for `jira sprint workitems query`.
#[derive(Default)]
pub struct SprintIssues<'a> {
    pub jql: Option<&'a str>,
    /// Extra fields to fetch and include.
    pub fields: &'a [String],
    pub expand: Option<&'a str>,
    pub start: usize,
    pub limit: usize,
}

/// A sprint ID from an ID, an ARI, or a URL with `sprint=N` / `/sprints/N`.
pub fn sprint_ref(s: &str) -> Result<u64> {
    let s = s.trim();
    if let Ok(n) = s.parse() {
        return Ok(n);
    }
    for marker in ["sprint=", "/sprints/", "/sprint/"] {
        if let Some(i) = s.find(marker) {
            let id: String = s[i + marker.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(n) = id.parse() {
                return Ok(n);
            }
        }
    }
    trailing_number(s)
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| anyhow!("'{s}' is not a sprint ID, ARI or URL"))
}

/// `jira sprint workitems query` (and the old `jira sprint issues`):
/// `{"issues": [...], "startAt", "maxResults", "total"}` with the same rows
/// as `jira workitem query` (flattened REST issues when `fields` is given).
pub fn sprint_issues(c: &Client, sprint_id: u64, o: &SprintIssues) -> Result<Value> {
    let limit = o.limit;
    let raw = !o.fields.is_empty();
    let field_param = if raw {
        o.fields.join(",")
    } else {
        LIST_FIELDS.join(",")
    };
    let mut issues = Vec::new();
    let mut total = None;
    while issues.len() < limit {
        let start = (o.start + issues.len()).to_string();
        let page_size = (limit - issues.len()).min(100).to_string();
        let mut q = vec![
            ("startAt", start.as_str()),
            ("maxResults", page_size.as_str()),
            ("fields", field_param.as_str()),
        ];
        if let Some(j) = o.jql {
            q.push(("jql", j));
        }
        if let Some(e) = o.expand {
            q.push(("expand", e));
        }
        let v = c.get(&format!("/rest/agile/1.0/sprint/{sprint_id}/issue"), &q)?;
        let batch = v["issues"].as_array().cloned().unwrap_or_default();
        let batch_len = batch.len();
        issues.extend(
            batch
                .iter()
                .take(limit - issues.len())
                .map(|i| issue_row(c, i, raw)),
        );
        total = v["total"].as_u64();
        if batch_len == 0
            || total.is_some_and(|t| (o.start + issues.len()) as u64 >= t)
            || (total.is_none() && batch_len < 100)
        {
            break;
        }
    }
    let mut out = json!({"issues": issues, "startAt": o.start, "maxResults": limit});
    if let Some(t) = total {
        out["total"] = t.into();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_fields_merge() {
        let m = extra_fields(
            Some(r#"{"customfield_1": 5}"#),
            &["customfield_2=\"7\"".into(), "x=hello".into()],
        )
        .unwrap();
        assert_eq!(m["customfield_1"], 5);
        assert_eq!(m["customfield_2"], "7");
        assert_eq!(m["x"], "hello");
        assert!(extra_fields(Some(r#"{"a":1}"#), &["a=2".into()]).is_err());
        assert!(extra_fields(None, &["novalue".into()]).is_err());
    }

    #[test]
    fn helpers() {
        assert_eq!(csv("a, b,,c"), vec!["a", "b", "c"]);
        assert_eq!(id_or_name("10001", "key"), json!({"id": "10001"}));
        assert_eq!(id_or_name("PROJ", "key"), json!({"key": "PROJ"}));
        let o = GetOpts {
            fields: Some(vec!["summary".into()]),
            extra: vec!["summary".into(), "customfield_1".into()],
            comments: false,
            remote_links: false,
            full: false,
        };
        assert_eq!(o.field_param(), "summary,customfield_1");
    }

    #[test]
    fn update_rejects_conflicting_list_edits() {
        let c = Client::new("https://example.atlassian.net".into(), "a@b.c", "tok", None);
        let err = update(
            &c,
            "PROJ-1",
            Update {
                labels: Some(ListEdit {
                    set: Some(vec!["a".into()]),
                    add: vec!["b".into()],
                    remove: vec![],
                }),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("labels"), "{err}");

        let err = update(
            &c,
            "PROJ-1",
            Update {
                components: Some(ListEdit {
                    set: Some(vec![]),
                    add: vec![],
                    remove: vec!["api".into()],
                }),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("components"), "{err}");

        let err = update(
            &c,
            "PROJ-1",
            Update {
                fix_versions: Some(ListEdit {
                    set: Some(vec!["1.0".into()]),
                    add: vec!["2.0".into()],
                    remove: vec![],
                }),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("fixVersions"), "{err}");
    }

    #[test]
    fn user_rows_follow_twg() {
        let with_email = json!({
            "accountId": "123",
            "displayName": "Alice",
            "emailAddress": "alice@example.com",
            "active": true,
            "accountType": "atlassian"
        });
        assert_eq!(
            user_row(&with_email),
            json!({
                "accountId": "123",
                "userAri": "ari:cloud:identity::user/123",
                "name": "Alice",
                "email": "alice@example.com",
                "sources": ["jira-rest"]
            })
        );
        let without_email = json!({"accountId": "456", "displayName": "Bob"});
        assert!(user_row(&without_email).get("email").is_none());

        let me = json!({"accountId": "123", "displayName": "Alice", "active": true,
                        "avatarUrls": {"48x48": "https://a/48"}, "locale": "en-US",
                        "timeZone": "Europe/Madrid", "emailAddress": "alice@example.com"});
        let p = profile(&me);
        assert_eq!(p["name"], "Alice");
        assert_eq!(p["picture"], "https://a/48");
        assert_eq!(p["accountStatus"], "active");
        assert_eq!(p["zoneinfo"], "Europe/Madrid");
        assert!(p["extendedProfile"]["jobTitle"].is_null());
    }

    #[test]
    fn filters_fields_by_query_and_custom() {
        let raw = vec![
            json!({"id": "summary", "name": "Summary", "custom": false, "schema": {"type": "string"}}),
            json!({"id": "customfield_10010", "name": "Story Points", "custom": true, "schema": {"type": "number"}}),
            json!({"id": "customfield_10020", "name": "Sprint", "custom": true, "schema": {"type": "array"}}),
        ];
        assert_eq!(filter_fields(&raw, None, false).len(), 3);
        let custom = filter_fields(&raw, None, true);
        assert_eq!(custom.len(), 2);
        assert_eq!(custom[0]["id"], "customfield_10010");
        assert_eq!(custom[0]["type"], "number");
        let by_name = filter_fields(&raw, Some("story"), true);
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0]["name"], "Story Points");
        let by_id = filter_fields(&raw, Some("10020"), false);
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0]["name"], "Sprint");
    }

    #[test]
    fn create_meta_helpers_resolve_and_sort_fields() {
        let types = vec![
            json!({"id": "10001", "name": "Task", "subtask": false, "description": "A task"}),
            json!({"id": "10002", "name": "Bug", "subtask": false, "description": "A bug"}),
        ];
        assert_eq!(resolve_issue_type(&types, "bug").unwrap()["id"], "10002");
        assert_eq!(resolve_issue_type(&types, "10001").unwrap()["name"], "Task");
        assert!(resolve_issue_type(&types, "Epic").is_err());

        let fields = vec![
            json!({"fieldId": "labels", "key": "labels", "name": "Labels", "required": false, "schema": {"type": "array"}}),
            json!({"fieldId": "customfield_10021", "key": "customfield_10021", "name": "Flagged", "required": false,
                   "schema": {"type": "array", "items": "option"}, "operations": ["add", "set", "remove"],
                   "allowedValues": [{"value": "Impediment", "id": "1", "iconUrl": "x"}]}),
            json!({"fieldId": "customfield_10015", "key": "customfield_10015", "name": "Start date", "required": false,
                   "schema": {"type": "date"}, "operations": ["set"]}),
        ];
        let out = create_fields(&fields);
        assert_eq!(out.len(), 2, "standard fields are left out");
        assert_eq!(out[0]["id"], "customfield_10015");
        assert_eq!(out[0]["acceptedInput"], "date");
        assert_eq!(out[0]["example"], "--field \"Start date=VALUE\"");
        assert_eq!(out[1]["acceptedInput"], "multi-option");
        assert_eq!(out[1]["example"], "--field \"Flagged=Impediment\"");
        assert_eq!(accepted_input(&json!({"type": "string"})), "text");
        assert_eq!(accepted_input(&json!({"type": "any"})), "value");
    }

    #[test]
    fn builds_link_payloads() {
        let link = build_issue_link_payload(
            "PROJ-1",
            "10002",
            "Blocks",
            Some(("blocked", TextFormat::Plain)),
        );
        assert_eq!(link["type"], json!({"name": "Blocks"}));
        assert_eq!(link["inwardIssue"], json!({"key": "PROJ-1"}));
        assert_eq!(link["outwardIssue"], json!({"id": "10002"}));
        assert_eq!(link["comment"]["body"]["type"], "doc");

        let link_no_comment = build_issue_link_payload("PROJ-1", "PROJ-2", "10000", None);
        assert_eq!(link_no_comment["type"], json!({"id": "10000"}));
        assert!(link_no_comment.get("comment").is_none());

        let remote = build_remote_link_payload(&Weblink {
            url: "https://example.com/doc",
            title: "Design Doc",
            relationship: Some("Wiki Page"),
            ..Default::default()
        });
        assert_eq!(
            remote,
            json!({"relationship": "Wiki Page", "object": {"url": "https://example.com/doc", "title": "Design Doc"}})
        );
        let remote_min = build_remote_link_payload(&Weblink {
            url: "https://example.com",
            title: "Home",
            ..Default::default()
        });
        assert!(remote_min.get("relationship").is_none());

        let full = build_remote_link_payload(&Weblink {
            url: "https://example.com/spec",
            title: "Spec",
            global_id: Some("spec-1"),
            application_name: Some("Docs"),
            application_type: Some("com.example.docs"),
            summary: Some("Design spec"),
            icon_url: Some("https://example.com/i.png"),
            resolved: true,
            ..Default::default()
        });
        assert_eq!(
            full,
            json!({
                "globalId": "spec-1",
                "application": {"name": "Docs", "type": "com.example.docs"},
                "object": {
                    "url": "https://example.com/spec",
                    "title": "Spec",
                    "summary": "Design spec",
                    "icon": {"url16x16": "https://example.com/i.png"},
                    "status": {"resolved": true},
                },
            })
        );
    }

    #[test]
    fn normalizes_sprint_states() {
        assert_eq!(
            normalize_sprint_states("active, future").unwrap(),
            "active,future"
        );
        assert_eq!(normalize_sprint_states("CLOSED").unwrap(), "closed");
        assert!(normalize_sprint_states("").is_err());
        assert!(normalize_sprint_states("active,unknown").is_err());
    }

    #[test]
    fn shapes_issues_and_pages_like_twg() {
        let c = Client::new(
            "https://acme.atlassian.net".into(),
            "u@acme.com",
            "tok",
            None,
        );
        assert_eq!(site(&c), "acme.atlassian.net");
        let issue = json!({
            "id": "10001", "key": "PROJ-1", "self": "https://acme.atlassian.net/rest/api/3/issue/10001",
            "fields": {
                "summary": "First",
                "status": {"id": "3", "name": "Done", "iconUrl": "x",
                           "statusCategory": {"id": 3, "key": "done", "name": "Done", "colorName": "green"}},
                "issuetype": {"id": "1", "name": "Task", "iconUrl": "x"},
                "assignee": null,
                "reporter": {"accountId": "a1", "displayName": "Ann", "avatarUrls": {"48x48": "x"}},
                "project": {"id": "100", "key": "PROJ", "name": "Project"},
                "created": "2026-01-01", "updated": "2026-01-02"
            }
        });
        let flat = flatten(&c, &issue);
        assert_eq!(flat["summary"], "First");
        assert_eq!(flat["url"], "https://acme.atlassian.net/browse/PROJ-1");
        assert!(flat["status"].get("iconUrl").is_none());
        assert!(flat["reporter"].get("avatarUrls").is_none());
        assert!(flat.get("fields").is_none());

        assert_eq!(
            row(&c, &issue),
            json!({
                "id": "10001", "key": "PROJ-1",
                "self": "https://acme.atlassian.net/rest/api/3/issue/10001",
                "url": "https://acme.atlassian.net/browse/PROJ-1",
                "summary": "First",
                "status": {"id": "3", "name": "Done", "statusCategory": {"key": "done", "name": "Done"}},
                "issueType": {"id": "1", "name": "Task"},
                "reporter": {"accountId": "a1", "displayName": "Ann"},
                "project": {"id": "100", "key": "PROJ", "name": "Project"},
                "created": "2026-01-01", "updated": "2026-01-02"
            })
        );
        assert_eq!(BoardType::Scrum.as_str(), "scrum");
        assert_eq!(BoardType::Kanban.as_str(), "kanban");

        let page = Paged {
            values: vec![json!({"id": 7, "avatarUrls": {}})],
            start: 0,
            limit: 50,
            total: Some(1),
            is_last: true,
            self_link: None,
        };
        assert_eq!(
            page.to_value("boards"),
            json!({"boards": [{"id": 7}], "total": 1, "startAt": 0, "maxResults": 50, "isLast": true})
        );
    }
}
