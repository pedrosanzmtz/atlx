//! Offline tests for the Jira triage commands (#23): comments, changelog,
//! edit metadata, link query and unlink, delete, and the type, status and
//! priority lookups.

mod common;

use atlx::client::{ApiError, ErrorKind};
use atlx::jira::{self, WeblinkRef};
use atlx::markup::TextFormat;
use common::{MockResponse, MockServer};
use serde_json::json;

#[test]
fn comment_query_returns_one_page_with_a_cursor() {
    let srv = MockServer::with_responses([
        MockResponse::json(
            200,
            json!({
                "startAt": 2, "maxResults": 2, "total": 5,
                "comments": [
                    {"id": "10002", "author": {"displayName": "Ann", "avatarUrls": {"48x48": "x"}},
                     "body": {"type": "doc", "version": 1, "content": []}},
                    {"id": "10003", "author": {"displayName": "Bo"}}
                ]
            }),
        ),
        MockResponse::json(
            200,
            json!({"startAt": 4, "maxResults": 2, "total": 5, "comments": [{"id": "10004"}]}),
        ),
    ]);
    let c = srv.client();

    let page = jira::comment_query(&c, "PROJ-1", 2, 2, Some("-created")).unwrap();
    assert_eq!(page.data.as_array().unwrap().len(), 2);
    assert_eq!(page.data[0]["id"], "10002");
    assert!(page.data[0]["author"].get("avatarUrls").is_none());
    assert_eq!(page.data[0]["body"]["type"], "doc");
    assert_eq!(page.next.as_deref(), Some("4"));
    assert_eq!(
        page.page_info(),
        json!({"hasNextPage": true, "nextCursor": "4"})
    );
    assert_eq!(
        page.pagination(),
        json!({"startAt": 2, "maxResults": 2, "total": 5})
    );

    let last = jira::comment_query(&c, "PROJ-1", 4, 2, None).unwrap();
    assert!(last.next.is_none());
    assert_eq!(
        last.page_info(),
        json!({"hasNextPage": false, "nextCursor": null})
    );

    let reqs = srv.requests();
    assert_eq!(reqs[0].path, "/rest/api/3/issue/PROJ-1/comment");
    assert_eq!(reqs[0].query_param("startAt").as_deref(), Some("2"));
    assert_eq!(reqs[0].query_param("maxResults").as_deref(), Some("2"));
    assert_eq!(reqs[0].query_param("orderBy").as_deref(), Some("-created"));
    assert!(reqs[1].query_param("orderBy").is_none());
}

#[test]
fn offset_cursors_must_be_numbers() {
    assert_eq!(jira::offset_cursor(None).unwrap(), 0);
    assert_eq!(jira::offset_cursor(Some(" 40 ")).unwrap(), 40);
    let err = jira::offset_cursor(Some("abc")).unwrap_err();
    assert_eq!(ApiError::find(&err).unwrap().kind, ErrorKind::Usage);
}

#[test]
fn changelog_query_passes_rest_entries_through() {
    let srv = MockServer::with_responses([MockResponse::json(
        200,
        json!({
            "startAt": 0, "maxResults": 100, "total": 1, "isLast": true,
            "values": [{
                "id": "10032",
                "author": {"accountId": "a1", "avatarUrls": {"16x16": "x"}},
                "created": "2026-01-01T00:00:00.000+0000",
                "items": [{"field": "summary", "fromString": "Old", "toString": "New"}]
            }]
        }),
    )]);
    let c = srv.client();
    let page = jira::changelog_query(&c, "PROJ-1", 0, 100).unwrap();
    assert_eq!(page.data[0]["items"][0]["toString"], "New");
    assert!(page.data[0]["author"].get("avatarUrls").is_none());
    assert!(page.next.is_none());
    let reqs = srv.requests();
    assert_eq!(reqs[0].path, "/rest/api/3/issue/PROJ-1/changelog");
    assert_eq!(reqs[0].query_param("maxResults").as_deref(), Some("100"));
}

#[test]
fn comment_update_delete_and_reply() {
    let srv = MockServer::with_responses([
        MockResponse::json(200, json!({"id": "10001", "author": {"avatarUrls": {}}})),
        MockResponse::empty(204),
        MockResponse::json(201, json!({"id": "10009"})),
    ]);
    let c = srv.client();

    let up = jira::comment_update(&c, "PROJ-1", "10001", "Edited", TextFormat::Plain).unwrap();
    assert_eq!(up["id"], "10001");
    assert_eq!(up["issueKey"], "PROJ-1");
    assert!(up["author"].get("avatarUrls").is_none());
    assert!(
        up["url"]
            .as_str()
            .unwrap()
            .ends_with("/browse/PROJ-1?focusedCommentId=10001")
    );

    let del = jira::comment_delete(&c, "PROJ-1", "10001").unwrap();
    assert_eq!(
        del,
        json!({"id": "10001", "issueId": "PROJ-1", "deleted": true})
    );

    let reply = jira::comment(
        &c,
        "PROJ-1",
        "Thanks",
        TextFormat::Plain,
        None,
        false,
        Some("10001"),
    )
    .unwrap();
    assert_eq!(reply["id"], "10009");

    let reqs = srv.requests();
    assert_eq!(reqs[0].method, "PUT");
    assert_eq!(reqs[0].path, "/rest/api/3/issue/PROJ-1/comment/10001");
    assert_eq!(reqs[0].json_body()["body"]["type"], "doc");
    assert_eq!(reqs[1].method, "DELETE");
    assert_eq!(reqs[1].path, "/rest/api/3/issue/PROJ-1/comment/10001");
    assert_eq!(reqs[2].method, "POST");
    assert_eq!(reqs[2].json_body()["parentId"], 10001);
}

#[test]
fn delete_sends_delete_subtasks() {
    let srv = MockServer::with_responses([MockResponse::empty(204), MockResponse::empty(204)]);
    let c = srv.client();
    let out = jira::delete(&c, "PROJ-1", false).unwrap();
    assert_eq!(
        out,
        json!({"id": "PROJ-1", "deleted": true, "deleteSubtasks": false})
    );
    jira::delete(&c, "PROJ-2", true).unwrap();
    let reqs = srv.requests();
    assert_eq!(reqs[0].method, "DELETE");
    assert_eq!(reqs[0].path, "/rest/api/3/issue/PROJ-1");
    assert_eq!(
        reqs[0].query_param("deleteSubtasks").as_deref(),
        Some("false")
    );
    assert_eq!(
        reqs[1].query_param("deleteSubtasks").as_deref(),
        Some("true")
    );
}

#[test]
fn update_meta_shapes_editable_fields() {
    let editmeta = json!({"fields": {
        "summary": {"required": true, "schema": {"type": "string", "system": "summary"},
                    "name": "Summary", "key": "summary", "operations": ["set"]},
        "issuetype": {"required": true, "schema": {"type": "issuetype", "system": "issuetype"},
                      "name": "Work type", "key": "issuetype", "operations": [], "allowedValues": []},
        "customfield_10021": {"required": false,
            "schema": {"type": "array", "items": "option", "custom": "x:multicheckboxes", "customId": 10021},
            "name": "Flagged", "key": "customfield_10021", "operations": ["add", "set", "remove"],
            "allowedValues": [{"self": "s", "value": "Impediment", "id": "1", "iconUrl": "i"}]},
        "priority": {"required": false, "schema": {"type": "priority", "system": "priority"},
                     "name": "Priority", "key": "priority", "operations": ["set"],
                     "allowedValues": [{"name": "Highest", "id": "1"}]},
        "labels": {"required": false, "schema": {"type": "array", "items": "string", "system": "labels"},
                   "name": "Labels", "key": "labels", "autoCompleteUrl": "u", "operations": ["add", "set", "remove"]},
        "comment": {"required": false, "schema": {"type": "comments-page", "system": "comment"},
                    "name": "Comment", "key": "comment", "operations": ["add", "edit", "remove"]}
    }});
    let srv = MockServer::with_responses([
        MockResponse::json(200, editmeta.clone()),
        MockResponse::json(200, editmeta),
    ]);
    let c = srv.client();

    let custom = jira::update_meta(&c, "PROJ-1", false).unwrap();
    let f = custom["fields"].as_array().unwrap();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0]["id"], "customfield_10021");
    assert_eq!(f[0]["acceptedInput"], "multi-option");
    assert_eq!(f[0]["example"], "--field \"Flagged=Impediment\"");
    assert!(f[0]["allowedValues"][0].get("iconUrl").is_none());

    let all = jira::update_meta(&c, "PROJ-1", true).unwrap();
    let ids: Vec<&str> = all["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    // REST order; issuetype has no operations, so it is left out.
    assert_eq!(
        ids,
        [
            "summary",
            "customfield_10021",
            "priority",
            "labels",
            "comment"
        ]
    );
    let by = |id: &str| {
        all["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(by("summary")["acceptedInput"], "value");
    assert_eq!(by("labels")["acceptedInput"], "value");
    assert!(by("labels").get("autoCompleteUrl").is_none());
    assert_eq!(by("priority")["acceptedInput"], "option");
    assert_eq!(by("priority")["example"], "--field \"Priority=Highest\"");
    assert!(
        by("comment").get("example").is_none(),
        "not settable, so no example"
    );
    assert_eq!(srv.requests()[0].path, "/rest/api/3/issue/PROJ-1/editmeta");
}

#[test]
fn link_query_lists_issue_links() {
    let srv = MockServer::with_responses([
        MockResponse::json(
            200,
            json!({"key": "PROJ-1", "fields": {"issuelinks": [{
                "id": "10500",
                "type": {"name": "Blocks", "inward": "is blocked by", "outward": "blocks"},
                "outwardIssue": {"key": "PROJ-2", "fields": {"status": {"name": "Open", "iconUrl": "i"}}}
            }]}}),
        ),
        MockResponse::json(200, json!({"key": "PROJ-3", "fields": {}})),
    ]);
    let c = srv.client();
    let links = jira::link_query(&c, "PROJ-1").unwrap();
    assert_eq!(links[0]["id"], "10500");
    assert_eq!(links[0]["outwardIssue"]["key"], "PROJ-2");
    assert!(
        links[0]["outwardIssue"]["fields"]["status"]
            .get("iconUrl")
            .is_none()
    );
    assert_eq!(jira::link_query(&c, "PROJ-3").unwrap(), json!([]));
    let reqs = srv.requests();
    assert_eq!(reqs[0].path, "/rest/api/3/issue/PROJ-1");
    assert_eq!(reqs[0].query_param("fields").as_deref(), Some("issuelinks"));
}

#[test]
fn unlink_workitem_and_weblink_by_each_selector() {
    let remote = json!([
        {"id": 7, "object": {"url": "https://example.com/a", "title": "A"}},
        {"id": 8, "object": {"url": "https://example.com/b", "title": "B"}},
        {"id": 9, "object": {"url": "https://example.com/b/", "title": "B again"}}
    ]);
    let srv = MockServer::with_responses([
        MockResponse::empty(204),
        MockResponse::empty(204),
        MockResponse::empty(204),
        MockResponse::json(200, remote.clone()),
        MockResponse::empty(204),
        MockResponse::json(200, remote.clone()),
        MockResponse::json(200, remote),
    ]);
    let c = srv.client();

    assert_eq!(
        jira::unlink_workitem(&c, "10500").unwrap(),
        json!({"linkId": "10500", "deleted": true})
    );
    let by_id = jira::unlink_weblink(&c, "PROJ-1", WeblinkRef::Id("7")).unwrap();
    assert_eq!(by_id["linkId"], "7");
    let by_global = jira::unlink_weblink(&c, "PROJ-1", WeblinkRef::GlobalId("g-1")).unwrap();
    assert_eq!(by_global["globalId"], "g-1");
    let by_url =
        jira::unlink_weblink(&c, "PROJ-1", WeblinkRef::Url("https://example.com/a")).unwrap();
    assert_eq!(by_url["linkId"], "7");
    assert_eq!(by_url["deleted"], true);

    let missing =
        jira::unlink_weblink(&c, "PROJ-1", WeblinkRef::Url("https://example.com/z")).unwrap_err();
    assert_eq!(ApiError::find(&missing).unwrap().kind, ErrorKind::NotFound);
    let several =
        jira::unlink_weblink(&c, "PROJ-1", WeblinkRef::Url("https://example.com/b")).unwrap_err();
    assert_eq!(ApiError::find(&several).unwrap().kind, ErrorKind::Usage);

    let reqs = srv.requests();
    assert_eq!(
        reqs.len(),
        7,
        "nothing is deleted when the URL is ambiguous"
    );
    assert_eq!(reqs[0].method, "DELETE");
    assert_eq!(reqs[0].path, "/rest/api/3/issueLink/10500");
    assert_eq!(reqs[1].path, "/rest/api/3/issue/PROJ-1/remotelink/7");
    assert_eq!(reqs[2].path, "/rest/api/3/issue/PROJ-1/remotelink");
    assert_eq!(reqs[2].query_param("globalId").as_deref(), Some("g-1"));
    assert_eq!(reqs[3].method, "GET");
    assert_eq!(reqs[4].method, "DELETE");
    assert_eq!(reqs[4].path, "/rest/api/3/issue/PROJ-1/remotelink/7");
}

#[test]
fn issue_types_for_a_project_and_by_id() {
    let srv = MockServer::with_responses([
        MockResponse::json(200, json!({"id": "10000", "key": "PROJ"})),
        MockResponse::json(
            200,
            json!([
                {"id": "1", "name": "Task", "iconUrl": "i", "hierarchyLevel": 0},
                {"id": "2", "name": "Bug", "hierarchyLevel": 0}
            ]),
        ),
        MockResponse::json(200, json!({"id": "10001", "name": "Epic", "iconUrl": "i"})),
    ]);
    let c = srv.client();
    let (key, id) = jira::project_key_id(&c, "PROJ").unwrap();
    assert_eq!((key.as_str(), id.as_str()), ("PROJ", "10000"));
    let (types, found) = jira::issue_types(&c, &id, Some(0), 1).unwrap();
    assert_eq!(found, 2);
    assert_eq!(
        types,
        json!([{"id": "1", "name": "Task", "hierarchyLevel": 0}])
    );
    let got = jira::issue_types_get(
        &c,
        &[jira::entity_id("ari:cloud:jira:abc:issue-type/10001")],
    )
    .unwrap();
    assert_eq!(got, json!([{"id": "10001", "name": "Epic"}]));

    let reqs = srv.requests();
    assert_eq!(reqs[0].path, "/rest/api/3/project/PROJ");
    assert_eq!(reqs[1].path, "/rest/api/3/issuetype/project");
    assert_eq!(reqs[1].query_param("projectId").as_deref(), Some("10000"));
    assert_eq!(reqs[1].query_param("level").as_deref(), Some("0"));
    assert_eq!(reqs[2].path, "/rest/api/3/issuetype/10001");
}

#[test]
fn statuses_and_priorities_all_or_by_id() {
    let srv = MockServer::with_responses([
        MockResponse::json(
            200,
            json!([{"id": "3", "name": "In Progress", "iconUrl": "i"}]),
        ),
        MockResponse::json(200, json!({"id": "3", "name": "In Progress"})),
        MockResponse::json(200, json!({"id": "10000", "name": "To Do"})),
        MockResponse::json(200, json!([{"id": "1", "name": "Highest", "iconUrl": "i"}])),
        MockResponse::json(200, json!({"id": "2", "name": "High"})),
    ]);
    let c = srv.client();
    assert_eq!(
        jira::statuses(&c, &[]).unwrap(),
        json!([{"id": "3", "name": "In Progress"}])
    );
    let some = jira::statuses(&c, &["3".into(), "10000".into()]).unwrap();
    assert_eq!(some[1]["name"], "To Do");
    assert_eq!(
        jira::priorities(&c, &[]).unwrap(),
        json!([{"id": "1", "name": "Highest"}])
    );
    assert_eq!(
        jira::priorities(&c, &["2".into()]).unwrap()[0]["name"],
        "High"
    );

    let paths: Vec<String> = srv.requests().into_iter().map(|r| r.path).collect();
    assert_eq!(
        paths,
        [
            "/rest/api/3/status",
            "/rest/api/3/status/3",
            "/rest/api/3/status/10000",
            "/rest/api/3/priority",
            "/rest/api/3/priority/2",
        ]
    );
}
