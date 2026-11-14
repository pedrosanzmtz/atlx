# twg compatibility

**Goal: atlx behaves exactly like twg** for Jira, Confluence, Bitbucket, users
and Jira Service Management. That means the same command paths, positional
arguments and flag names, the same `{"data": ...}` JSON envelope and key names,
and no confirmation gates where twg has none (`--yes` stays accepted as a
no-op). Commands that need Atlassian's Teamwork Graph, the GraphQL gateway or
Rovo are out of scope; atlx is built only on the public REST APIs. These rules
come from the maintainer decisions on
[#18](https://github.com/pedrosanzmtz/atlx/issues/18) and
[#21](https://github.com/pedrosanzmtz/atlx/issues/21).

This page compares atlx with **twg 1.4.4**. The twg side comes from
[`compat/twg-1.4.4.txt`](compat/twg-1.4.4.txt), made by
[`scripts/twg-inventory.sh`](scripts/twg-inventory.sh), which records command
paths, positionals and flag names only. Coverage per product is in
[`compat/coverage.txt`](compat/coverage.txt).

## Status values

| Status | Meaning |
| --- | --- |
| ✅ | Supported under twg's path, with all of twg's flags that apply |
| 🟡 | Supported under twg's path, but flags or behavior are missing (listed, with the issue) |
| 🔁 | Accepted under twg's path through an alias of a differently named atlx command |
| ➕ | atlx only; twg 1.4.4 has no such command |
| 📋 | Planned; the note links the issue. `#13 (unscheduled)` means public REST covers it but no issue is open yet |
| ⛔ | Out of scope: needs Teamwork Graph, the GraphQL gateway, Rovo or Atlassian AI, or is specific to twg's OAuth setup |

Flags that only feed twg's GraphQL transport (`--variables-json`,
`--cloud-id`, `--*-input`, ...) are listed as "not planned" and do not make a
row 🟡. Bitbucket's `--workspace`/`--repo`, which twg declares on the
`bitbucket` group, are accepted on every atlx Bitbucket command.

## How this page is kept accurate

`cargo test --test twg_compat` runs offline on every CI platform and checks:

- **This table against the parser.** Every twg leaf command in the inventory
  needs a row. ✅ and 🟡 commands must exist in
  [`tests/snapshots/cli-tree.txt`](tests/snapshots/cli-tree.txt), 🔁 commands
  must exist only through an alias, and 📋 and ⛔ commands must not parse.
- **The parse corpus**, [`compat/corpus.txt`](compat/corpus.txt): twg-style
  invocations tagged `ok` (must parse) or `gap #<issue>` (must fail to parse).
  When a change makes a gap line parse, the test fails until the line is
  flipped to `ok` and the row here is updated.
- **Coverage**, which may only go up. Record a rise with
  `UPDATE_COVERAGE=1 cargo test --test twg_compat`.

These checks are about parsing. Output shapes are covered by
[`tests/twg_shapes.rs`](tests/twg_shapes.rs) and the
[JSON output shapes](#json-output-shapes) section below.

## Global flags and behavior

| twg | atlx | Notes |
| --- | --- | --- |
| `-o, --output text\|json\|jsonl` | ✅ | atlx defaults to JSON when stdout is piped |
| `-s, --site` | ✅ | |
| `-u, --user` | ✅ | |
| `--timeout-ms` | ✅ | |
| `-p, --profile` | ✅ | named profiles ([#20](https://github.com/pedrosanzmtz/atlx/issues/20)) |
| `--select`, `--agent-fields`, `--output-summary` | ✅ | output controls ([#19](https://github.com/pedrosanzmtz/atlx/issues/19)); see the README |
| `--api-version` | ✅ | `v2` is accepted and other versions are rejected ([#19](https://github.com/pedrosanzmtz/atlx/issues/19)) |
| `--output-file` | ➕ | twg 1.4.4 lists no such global flag; atlx writes stdout to a file |
| `--usage-counts` | ⛔ | usage counts come from the GraphQL gateway |
| `--output-shape` | ⛔ | the shapes reshape Teamwork Graph relationships |
| `{"data": ...}` JSON envelope and key names | ✅ | `apiVersion`, `command`, `request`, `data`, `pageInfo`, `meta` as twg prints them; Bitbucket unwrapped, as in twg ([#21](https://github.com/pedrosanzmtz/atlx/issues/21)); see [JSON output shapes](#json-output-shapes) |
| JSONL rows with `_twg` locators | ✅ | one line per element of the main collection ([#21](https://github.com/pedrosanzmtz/atlx/issues/21)) |
| Teamwork Graph, Rovo and GraphQL-only commands | ⛔ | fail with "not supported by atlx (requires Teamwork Graph)" in twg's error envelope, `category: "unsupported"`, exit code 2; 📋 commands fail with a pointer to their issue ([#21](https://github.com/pedrosanzmtz/atlx/issues/21)) |
| Exit codes and the JSON error envelope | ✅ | [#17](https://github.com/pedrosanzmtz/atlx/issues/17) |
| No `--yes` on merge, decline and content delete | ✅ | `--yes` is still accepted and ignored; nothing asks for confirmation ([#18](https://github.com/pedrosanzmtz/atlx/issues/18)) |
| API-token auth | ➕ | twg uses OAuth; atlx uses Atlassian and Bitbucket API tokens |

## JSON output shapes

Maintainer decision on [#21](https://github.com/pedrosanzmtz/atlx/issues/21):
**atlx behaves as twg.** The default JSON and JSONL output uses twg's
envelope and key names; there is no separate compatibility mode. Commands
that need Teamwork Graph or Rovo fail with "not supported by atlx (requires
Teamwork Graph)".

`scripts/conformance.sh` runs read-only commands through twg 1.4.4 and atlx
against the same site and reports every key path only one side prints. The
run for #21 used a site with Jira but no Confluence and no Bitbucket
repositories, so:

| Area | How the shape was established | Result |
| --- | --- | --- |
| `whoami`, `user search` (with and without `--compact`) | Live side by side | Same key paths. `whoami` data comes from Jira's `myself`: `zoneinfo` is the Jira time zone (twg prints `null`) and `extendedProfile` is all `null`; profile, site and token source are in `meta` |
| Jira `workitem get` (default, `--comments`, `--fields`, `--full`, several IDs), `workitem query` (rows, `--fields`, paging), `workitem search`, `transitions query`, `field create-metadata`, `link-types query`, `space query`, `space get`, `board query` | Live side by side | Same key paths and types. Differences in values only: `self` links use the site URL (twg goes through `api.atlassian.com`), and `meta` describes atlx's REST source instead of twg's backend |
| Jira `board sprints query`, `sprint workitems query` | Inferred: the site has no sprints. Follows the pattern twg uses for the other Agile lists (REST page, `values` renamed `sprints`; work item rows as in `workitem query`) | Not verified |
| Jira `workitem comment query`, `changelog query` (default, `--first`, `--after`, `--order-by`), `field update-metadata` (with and without `--include-system-fields`), `types query` (`--project`, `--issue --level -n`, `--id`), `types get`, `statuses query`, `priorities query` (all, and by `--id`) | Live side by side ([#23](https://github.com/pedrosanzmtz/atlx/issues/23)) | Same key paths, and the same `data` values once the `self` host is normalised. Comment and changelog pages print `pageInfo` on the last page too (`hasNextPage: false`, `nextCursor: null`); `types query` adds top-level `warnings` when `--limit` cut the list or `--id` was used |
| Jira `workitem link query` | Live, but the site's work items have no links, so only the empty list was compared. With links, the REST `issuelinks` entries are inferred | Partly verified |
| Jira writes (`create`, `update`, `comment create`, `transition`, `link workitem`, `link weblink`, `comment update`, `comment delete`, `delete`, `unlink workitem`, `unlink weblink`) | Inferred (the run was read-only): REST responses with camelCase atlx additions (`url`, `commentId`, ...); deletes print `{id, deleted: true}`-style objects | Not verified |
| Confluence `content get`, `search query`, `search text` | Inferred from twg's `--select` preset expansions (see `src/output/select.rs`): `id`, `type`, `title`, `status`, `spaceId`, `externalUrl`, `detail`, `snapshotToken`, `summary.{excerpt,wordCount,sectionCount}`, `outline[].{level,text,anchor}`, `body.{format,value}`; search passes REST's `results` through with `totalSize`, `size`, `nextCursor` | Not verified; `aiSummary` and `body.lossyConversion` are not produced. atlx adds `outline[].localId` and `outline[].wordCount`, and `body.sections` for `--section` reads. The `anchor` spelling (whitespace runs to `-`) is a guess |
| Confluence `content update --edits` | Inferred from twg's help: the dry run's `data.body.value` holds the resulting HTML | Not verified; atlx's dry run also has `fromVersion`, `request` and a unified `diff` |
| Other Confluence commands | Inferred: REST v2 objects with `externalUrl` and `snapshotToken`, lists as `{results, nextCursor}` | Not verified |
| Bitbucket | Partly live: twg printed `repo query` as a bare list and a failed batch `pull-requests get` as `{workspace, repository, items, summary, partial}`, both without an envelope. The rest is inferred from the presets: REST 2.0 objects plus `_comments`, `_statuses`, `_repo` and the step log fields | Not verified with data. One known difference: when every ID of a Bitbucket batch fails, twg still prints the batch object (exit 1) while atlx prints the error envelope (exit 1) |
| Account commands (`login`, `logout`, `auth profile ...`, `setup ...`, `doctor`) and `atlx search` | Envelope only. twg's data here describes its OAuth token store, which atlx doesn't have, so the data keeps atlx's own keys | Intentional |
| Errors | The error envelope from #17; JSONL error lines get `_twg: {collection: "root", index: 0}` as in twg | Live |

### Confluence targeted edits

`content update --edits <json>` and `--edits-file <path>` (`-` reads stdin)
take a JSON array of operations in twg's format: each has a `name`, the
target node's ADF `localId` and, where it adds content, an HTML `value`.
atlx reads the page with `body-format=atlas_doc_format`, applies every
operation to a copy of the ADF document, and sends the result back as
`atlas_doc_format` with the version number plus one. Nodes the operations
don't touch are sent back unchanged. If any operation fails (unknown
`localId`, two nodes with the same ID, a bad argument) the command exits
with a usage error and writes nothing. `--snapshot-token` is checked as for
full-body updates, and `--dry-run` prints the resulting HTML and a unified
diff without a PUT.

| Operation | Fields | Notes |
| --- | --- | --- |
| replaceNode | `localId`, `value` | The new nodes take the target's shape: inline content for an inline node, list items for a list item, rows for a row |
| insertNodeBefore, insertNodeAfter | `localId`, `value` | Same shaping as `replaceNode` |
| deleteNode | `localId` | |
| removeNodes | `arguments.localIds` | All IDs must exist |
| moveNode | `arguments.{localId, targetLocalId, position}` | `position` is `before` or `after` (default); twg's other positions, if any, are unknown |
| appendNodeToEnd | `value` | Appended to the end of the document |
| setAttrs | `localId`, `arguments.attrs` | Merged into the node's `attrs` |

Fields may sit at the top level or under `arguments`. Node `localId`s come
from `content get --detail outline` (headings) or the atlx extra
`content get --detail full --format adf` (every node). In twg the IDs come
from a full read; atlx's `--format html` is the storage format, which doesn't
carry ADF IDs, hence `adf`. The operation list is taken from twg's help and
has not been checked against a live site.

atlx extras, for pages whose nodes lack `localId`s, use `op` (or `name`) and
work on top-level headings. A heading query matches case-insensitively,
exact text first, and must select one heading:

| Operation | Fields |
| --- | --- |
| replaceText | `find`, `replace`, `count` (a number, default 1, or `"all"`). The match must sit inside one text run; no match, or more than `count`, is an error |
| replaceSection | `heading`, `value`, `format` (`html`, default, or `md`). Keeps the heading |
| appendToSection, prependToSection | `heading`, `value`, `format` |
| deleteSection | `heading`. Removes the heading too |
| insertSectionAfter | `heading`, `newHeading`, `level` (default: the anchor heading's), `value`, `format` |

`tests/twg_shapes.rs` replays the live-verified commands against synthetic
fixtures (`tests/fixtures/twg/`, made by `scripts/twg-fixtures.sh`, values
replaced and key names kept) so the shapes can't drift. Re-run the
conformance script on a site with Confluence and Bitbucket data to verify the
inferred rows.

## Jira

161 twg commands: ✅ 28, 🟡 2, 📋 121, ⛔ 10.

| twg command | atlx | Notes |
| --- | --- | --- |
| `jira board backlog query` | 📋 | #25; Agile 1.0 board backlog |
| `jira board backlog-view query` | ⛔ | board view model is only in the GraphQL gateway |
| `jira board cells query` | ⛔ | board view model is only in the GraphQL gateway |
| `jira board create` | 📋 | #25 |
| `jira board delete` | 📋 | #25 |
| `jira board get` | 📋 | #25 |
| `jira board projects query` | 📋 | #25; Agile 1.0 board projects |
| `jira board query` | ✅ | #18; `--issue` uses the work item's space; GraphQL-only `--variables-json` not planned |
| `jira board quick-filter get` | 📋 | #25; Agile 1.0 quick filters |
| `jira board quick-filter query` | 📋 | #25; Agile 1.0 quick filters |
| `jira board scope query` | ⛔ | board scope model is only in the GraphQL gateway |
| `jira board sprints query` | ✅ | #18; `--project`/`--issue` use the space's only board; GraphQL-only `--variables-json` not planned |
| `jira board view-settings query` | ⛔ | board view model is only in the GraphQL gateway |
| `jira dashboard bulk-edit` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard copy` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard create` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard delete` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard gadget add` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard gadget catalog query` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard gadget delete` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard gadget query` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard gadget update` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard get` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard item-property delete` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard item-property get` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard item-property query` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard item-property set` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard query` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira dashboard update` | 📋 | #13 (unscheduled); REST v3 dashboards |
| `jira field cancel-delete` | 📋 | #13 (unscheduled); REST v3 custom field admin |
| `jira field create` | 📋 | #13 (unscheduled); REST v3 custom field admin |
| `jira field delete` | 📋 | #13 (unscheduled); REST v3 custom field admin |
| `jira field update` | 📋 | #13 (unscheduled); REST v3 custom field admin |
| `jira filter add-favourite` | 📋 | #26 |
| `jira filter change-owner` | 📋 | #26 |
| `jira filter columns get` | 📋 | #26 |
| `jira filter columns set` | 📋 | #26 |
| `jira filter create` | 📋 | #26 |
| `jira filter delete` | 📋 | #26 |
| `jira filter get` | 📋 | #26 |
| `jira filter query` | 📋 | #26 |
| `jira filter remove-favourite` | 📋 | #26 |
| `jira filter reset-columns` | 📋 | #26 |
| `jira filter share add` | 📋 | #26 |
| `jira filter share query` | 📋 | #26 |
| `jira filter share remove` | 📋 | #26 |
| `jira filter subscription query` | 📋 | #26 |
| `jira filter update` | 📋 | #26 |
| `jira space archive` | 📋 | #26; REST v3 project endpoints |
| `jira space component counts` | 📋 | #26 |
| `jira space component create` | 📋 | #26 |
| `jira space component delete` | 📋 | #26 |
| `jira space component get` | 📋 | #26 |
| `jira space component query` | 📋 | #26 |
| `jira space component update` | 📋 | #26 |
| `jira space create` | 📋 | #26; REST v3 project endpoints |
| `jira space delete` | 📋 | #26; REST v3 project endpoints |
| `jira space get` | ✅ | #18; GraphQL-only `--cloud-id`, `--variables-json` not planned |
| `jira space issue-types` | 📋 | #26; REST v3 project endpoints |
| `jira space notification-scheme get` | 📋 | #26; REST v3 project endpoints |
| `jira space query` | ✅ | #18; `--after` takes a numeric offset (REST pages by number); GraphQL-only `--cloud-id`, `--input-json`, `--variables-json` not planned |
| `jira space restore` | 📋 | #26; REST v3 project endpoints |
| `jira space status query` | 📋 | #26; REST v3 project endpoints |
| `jira space types query` | 📋 | #26; REST v3 project endpoints |
| `jira space update` | 📋 | #26; REST v3 project endpoints |
| `jira space version counts` | 📋 | #26 |
| `jira space version create` | 📋 | #26 |
| `jira space version delete` | 📋 | #26 |
| `jira space version get` | 📋 | #26 |
| `jira space version merge` | 📋 | #26 |
| `jira space version move` | 📋 | #26 |
| `jira space version update` | 📋 | #26 |
| `jira space versions query` | 📋 | #26 |
| `jira sprint complete` | 📋 | #25; Agile 1.0 sprint endpoints |
| `jira sprint create` | 📋 | #25; Agile 1.0 sprint endpoints |
| `jira sprint delete` | 📋 | #25; Agile 1.0 sprint endpoints |
| `jira sprint get` | 📋 | #25; Agile 1.0 sprint endpoints |
| `jira sprint snapshot` | 📋 | #25 |
| `jira sprint start` | 📋 | #25; Agile 1.0 sprint endpoints |
| `jira sprint update` | 📋 | #25; Agile 1.0 sprint endpoints |
| `jira sprint workitems query` | ✅ | #18; GraphQL-only `--variables-json` not planned |
| `jira workitem archive` | 📋 | #13 (unscheduled); REST v3 issue archival |
| `jira workitem attachment delete` | 📋 | #24 |
| `jira workitem attachment download` | 📋 | #24 |
| `jira workitem attachment get` | 📋 | #24 |
| `jira workitem attachment query` | 📋 | #24 |
| `jira workitem attachment thumbnail` | 📋 | #24 |
| `jira workitem attachment upload` | 📋 | #24 |
| `jira workitem bulk-get` | 📋 | #13 (unscheduled); REST v3 bulkfetch |
| `jira workitem bulk-transition` | 📋 | #13 (unscheduled); REST v3 bulk transition |
| `jira workitem changelog query` | ✅ | #23; one REST page per call, `--after` takes the previous page's `pageInfo.nextCursor`; GraphQL-only `--variables-json` not planned |
| `jira workitem clone` | 📋 | #13 (unscheduled); read then create |
| `jira workitem comment create` | ✅ | `--parent-id` sends REST `parentId` (#23; inferred, not checked live); GraphQL-only `--variables-json` not planned |
| `jira workitem comment delete` | ✅ | #23; GraphQL-only `--variables-json` not planned |
| `jira workitem comment query` | ✅ | #23; one REST page per call, `--after` takes the previous page's `pageInfo.nextCursor`; GraphQL-only `--variables-json` not planned |
| `jira workitem comment update` | ✅ | #23; GraphQL-only `--variables-json` not planned |
| `jira workitem create` | ✅ | GraphQL-only `--variables-json` not planned |
| `jira workitem create-bulk` | 📋 | #13 (unscheduled); REST v3 bulk create |
| `jira workitem delete` | ✅ | #23; REST v3 issue delete, no confirmation as in twg; GraphQL-only `--input-json`, `--variables-json` not planned |
| `jira workitem field create-metadata` | ✅ | #18; also reachable as `jira workitem custom-fields create-metadata` |
| `jira workitem field update-metadata` | ✅ | #23; REST v3 editmeta, custom fields unless `--include-system-fields` |
| `jira workitem get` | ✅ | `--batch-concurrency` fetches in parallel and keeps input order (#18); GraphQL-only `--variables-json` not planned |
| `jira workitem link artifact` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link branch` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link build` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link commit` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link deployment` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link goal` | ⛔ | Atlassian Goals link (GraphQL gateway) |
| `jira workitem link loom` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link meeting` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link page` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link pr` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link project` | ⛔ | Atlassian Projects link (GraphQL gateway) |
| `jira workitem link query` | ✅ | #23; the work item's REST `issuelinks`; GraphQL-only `--variables-json` not planned |
| `jira workitem link repo` | 📋 | #13 (unscheduled); a remote link (REST v3 remotelink) |
| `jira workitem link weblink` | ✅ | #18 |
| `jira workitem link workitem` | ✅ | #18; `--id` is the side that reads with the outward phrase (`--id A --target-id B --link-type-id Blocks` means A blocks B), sent as REST `inwardIssue` |
| `jira workitem link-types query` | ✅ | #18; REST returns every link type at once, so the GraphQL paging flags `--after`, `--before`, `--first`, `--last` and `--cloud-id`, `--variables-json` are not planned |
| `jira workitem priorities query` | ✅ | #23; GraphQL-only `--variables-json` not planned |
| `jira workitem project-link-candidates query` | ⛔ | Atlassian Projects relationship (Teamwork Graph) |
| `jira workitem property delete` | 📋 | #13 (unscheduled); REST v3 issue properties |
| `jira workitem property get` | 📋 | #13 (unscheduled); REST v3 issue properties |
| `jira workitem property query` | 📋 | #13 (unscheduled); REST v3 issue properties |
| `jira workitem property set` | 📋 | #13 (unscheduled); REST v3 issue properties |
| `jira workitem query` | 🟡 | `--before` and `--last` parse but return a usage error: REST search only pages forward (#18); GraphQL-only `--cloud-id`, `--field-sets-input`, `--issue-search-input`, `--namespace`, `--options`, `--save-jqlto-user-history`, `--scope`, `--variables-json`, `--view-config-input` not planned |
| `jira workitem search` | ✅ |  |
| `jira workitem similar` | ⛔ | semantic similarity needs Atlassian AI |
| `jira workitem statuses query` | ✅ | #23; GraphQL-only `--variables-json` not planned |
| `jira workitem transition` | ✅ |  |
| `jira workitem transitions query` | ✅ |  |
| `jira workitem types get` | ✅ | #23; GraphQL-only `--variables-json` not planned |
| `jira workitem types query` | ✅ | #23; REST v3 `issuetype/project`; `--id` looks types up as `types get` does; GraphQL-only `--variables-json` not planned |
| `jira workitem unarchive` | 📋 | #13 (unscheduled); REST v3 issue archival |
| `jira workitem unlink artifact` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink branch` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink build` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink commit` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink deployment` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink goal` | ⛔ | Atlassian Goals link (GraphQL gateway) |
| `jira workitem unlink loom` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink meeting` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink page` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink pr` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink project` | ⛔ | Atlassian Projects link (GraphQL gateway) |
| `jira workitem unlink repo` | 📋 | #13 (unscheduled); delete a remote link (REST v3 remotelink) |
| `jira workitem unlink weblink` | ✅ | #23; `--url` deletes the one remote link pointing there |
| `jira workitem unlink workitem` | ✅ | #23; GraphQL-only `--variables-json` not planned |
| `jira workitem update` | 🟡 | missing `--original-estimate`, `--remaining-estimate`, `--sprint`, `--story-points`, `--team` (#25) |
| `jira workitem vote add` | 📋 | #24 |
| `jira workitem vote query` | 📋 | #24 |
| `jira workitem vote remove` | 📋 | #24 |
| `jira workitem watcher add` | 📋 | #24 |
| `jira workitem watcher query` | 📋 | #24 |
| `jira workitem watcher remove` | 📋 | #24 |
| `jira workitem worklog add` | 📋 | #24 |
| `jira workitem worklog changed` | 📋 | #24 |
| `jira workitem worklog delete` | 📋 | #24 |
| `jira workitem worklog deleted` | 📋 | #24 |
| `jira workitem worklog get` | 📋 | #24 |
| `jira workitem worklog query` | 📋 | #24 |
| `jira workitem worklog update` | 📋 | #24 |
| `jira board list` | ➕ | old name of `jira board query` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `jira field list` | ➕ | list fields with ids; twg's `jira field` is admin CRUD |
| `jira project get` | ➕ | old name of `jira space get` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `jira project list` | ➕ | old name of `jira space query` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `jira sprint issues` | ➕ | old name of `jira sprint workitems query` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `jira sprint list` | ➕ | old name of `jira board sprints query` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `jira workitem create-meta` | ➕ | old name of `jira workitem field create-metadata` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `jira workitem link create` | ➕ | old name of `jira workitem link workitem` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `jira workitem link remote` | ➕ | old name of `jira workitem link weblink` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `jira workitem link types` | ➕ | old name of `jira workitem link-types query` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |

## Confluence

121 twg commands: ✅ 7, 🟡 6, 📋 72, ⛔ 36.

| twg command | atlx | Notes |
| --- | --- | --- |
| `confluence approvals settings site get` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence approvals settings site set` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence approvals settings space get` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence approvals settings space set` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content analytics` | 📋 | #13 (unscheduled); v1 analytics views/viewers |
| `confluence content approvals approve` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals cancel` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals clear` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals due-date clear` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals due-date set` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals enabled` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals get` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals history` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals message set` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals pending` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals request` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals request-changes` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content approvals reviewers update` | ⛔ | page approvals are only in the GraphQL gateway |
| `confluence content archive` | 📋 | #30 |
| `confluence content attachments delete` | 📋 | #28 |
| `confluence content attachments download` | 📋 | #28 |
| `confluence content attachments get` | 📋 | #28 |
| `confluence content attachments list` | 📋 | #28 |
| `confluence content attachments upload` | 📋 | #28 |
| `confluence content body-formats` | 📋 | #28 |
| `confluence content comments create` | 🟡 | missing `--reference-space`, `--text-selection`, `--text-selection-match-count`, `--text-selection-match-index` (#29); `--comment-type inline` returns a usage error until #29. The old positional `<ref>` and `--format` still work (hidden, deprecated) |
| `confluence content comments delete` | 📋 | #29 |
| `confluence content comments get` | 📋 | #29 |
| `confluence content comments list` | 🟡 | missing `--content-type`, `--include-replies`, `--reference-space`, `--resolution-status`, `--sort`, `--status` (#29). The old positional `<ref>`, `--inline` and `--format` still work (hidden, deprecated) |
| `confluence content comments reopen` | 📋 | #29 |
| `confluence content comments reply` | 📋 | #29 |
| `confluence content comments resolve` | 📋 | #29 |
| `confluence content comments update` | 📋 | #29 |
| `confluence content convert` | 📋 | #28; v1 contentbody/convert |
| `confluence content copy` | 📋 | #30 |
| `confluence content create` | 🟡 | missing `--content-width`, `--private`, `--reference-space`, `--synced-container-url`, `--title-emoji` (#30) |
| `confluence content delete-draft` | 📋 | #30 |
| `confluence content disable-public-link` | ⛔ | public links are only in the GraphQL gateway |
| `confluence content enable-public-link` | ⛔ | public links are only in the GraphQL gateway |
| `confluence content export` | ⛔ | no public PDF/Word export endpoint |
| `confluence content export-status` | ⛔ | no public PDF/Word export endpoint |
| `confluence content get` | ✅ | `--detail ai_summary` is not offered (Atlassian AI); `--format adf` is an atlx extra that shows node `localId`s for `--edits` |
| `confluence content get-public-link` | ⛔ | public links are only in the GraphQL gateway |
| `confluence content history diff` | 📋 | #28 |
| `confluence content history get` | 📋 | #28 |
| `confluence content history list` | 📋 | #28 |
| `confluence content labels add` | 📋 | #28 |
| `confluence content labels list` | 🟡 | missing `--content-type`, `--reference-space` (#28) |
| `confluence content labels remove` | 📋 | #28 |
| `confluence content labels unwatch` | 📋 | #28 |
| `confluence content labels watch` | 📋 | #28 |
| `confluence content list` | ✅ |  |
| `confluence content list-available-statuses` | 📋 | #13 (unscheduled); v1 content states |
| `confluence content macros resolve` | 📋 | #13 (unscheduled); v1 async body conversion, to confirm |
| `confluence content macros resolve-status` | 📋 | #13 (unscheduled); v1 async body conversion, to confirm |
| `confluence content move` | 📋 | #30 |
| `confluence content permissions add` | 📋 | #13 (unscheduled); v1 content restrictions |
| `confluence content permissions check` | 📋 | #13 (unscheduled); v1 content restrictions |
| `confluence content permissions clear` | 📋 | #13 (unscheduled); v1 content restrictions |
| `confluence content permissions list` | 📋 | #13 (unscheduled); v1 content restrictions |
| `confluence content permissions remove` | 📋 | #13 (unscheduled); v1 content restrictions |
| `confluence content permissions replace` | 📋 | #13 (unscheduled); v1 content restrictions |
| `confluence content publish` | 📋 | #30; v2 draft to current |
| `confluence content purge` | 📋 | #30; atlx today: `confluence content delete --purge` |
| `confluence content reactions add` | ⛔ | reactions are only in the GraphQL gateway |
| `confluence content reactions list` | ⛔ | reactions are only in the GraphQL gateway |
| `confluence content reactions remove` | ⛔ | reactions are only in the GraphQL gateway |
| `confluence content restriction-state get` | 📋 | #13 (unscheduled); v1 content restrictions |
| `confluence content restriction-state set` | 📋 | #13 (unscheduled); v1 content restrictions |
| `confluence content set-owner` | 📋 | #13 (unscheduled); public endpoint to confirm |
| `confluence content star` | ⛔ | starring is only in the GraphQL gateway |
| `confluence content status get` | 📋 | #13 (unscheduled); v1 content states |
| `confluence content status set` | 📋 | #13 (unscheduled); v1 content states |
| `confluence content tasks complete` | 📋 | #13 (unscheduled); v2 tasks |
| `confluence content tasks get` | 📋 | #13 (unscheduled); v2 tasks |
| `confluence content tasks list` | 📋 | #13 (unscheduled); v2 tasks |
| `confluence content tasks reopen` | 📋 | #13 (unscheduled); v2 tasks |
| `confluence content trash` | 📋 | #30; atlx today: `confluence content delete` |
| `confluence content unarchive` | 📋 | #30 |
| `confluence content unstar` | ⛔ | starring is only in the GraphQL gateway |
| `confluence content untrash` | 📋 | #30 |
| `confluence content unwatch` | 📋 | #13 (unscheduled); v1 user/watch |
| `confluence content update` | 🟡 | missing `--content-width`, `--header-image-file`, `--header-image-file-id`, `--header-image-position`, `--header-image-url`, `--reference-space`, `--remove-header-image` (#30). `--context-content`/`--context-content-file` are not planned: they only apply to whiteboard and database bodies, which public REST doesn't return. `--edits`/`--edits-file` take twg's operations (see [Confluence targeted edits](#confluence-targeted-edits)) |
| `confluence content versions diff` | 📋 | #28 |
| `confluence content versions get` | 📋 | #28 |
| `confluence content versions list` | 📋 | #28 |
| `confluence content versions restore` | 📋 | #28 |
| `confluence content watch` | 📋 | #13 (unscheduled); v1 user/watch |
| `confluence group get` | 📋 | #13 (unscheduled); v1 groups |
| `confluence remix infographic create` | ⛔ | AI generation (Atlassian AI) |
| `confluence remix infographic edit` | ⛔ | AI generation (Atlassian AI) |
| `confluence remix maui create` | ⛔ | AI generation (Atlassian AI) |
| `confluence remix maui edit` | ⛔ | AI generation (Atlassian AI) |
| `confluence remix maui get` | ⛔ | AI generation (Atlassian AI) |
| `confluence search query` | ✅ |  |
| `confluence search text` | ✅ |  |
| `confluence space archive` | 📋 | #30 |
| `confluence space create` | 📋 | #30 |
| `confluence space delete` | 📋 | #30 |
| `confluence space get` | ✅ | `--labels` adds a `labels` array (#18) |
| `confluence space instructions get` | ⛔ | Rovo space instructions |
| `confluence space instructions set` | ⛔ | Rovo space instructions |
| `confluence space list` | ✅ | `--labels` adds a `labels` array to each space (#18) |
| `confluence space me` | ✅ |  |
| `confluence space permission available` | 📋 | #13 (unscheduled); v2 space permissions |
| `confluence space permission list` | 📋 | #13 (unscheduled); v2 space permissions |
| `confluence space role assignment list` | 📋 | #13 (unscheduled); v2 space roles |
| `confluence space role create` | 📋 | #13 (unscheduled); v2 space roles |
| `confluence space role get` | 📋 | #13 (unscheduled); v2 space roles |
| `confluence space role list` | 📋 | #13 (unscheduled); v2 space roles |
| `confluence space role mode` | 📋 | #13 (unscheduled); v2 space roles |
| `confluence space role update` | 📋 | #13 (unscheduled); v2 space roles |
| `confluence space star` | ⛔ | starring is only in the GraphQL gateway |
| `confluence space unarchive` | 📋 | #30 |
| `confluence space unstar` | ⛔ | starring is only in the GraphQL gateway |
| `confluence space unwatch` | 📋 | #13 (unscheduled); v1 user/watch |
| `confluence space update` | 📋 | #30 |
| `confluence space watch` | 📋 | #13 (unscheduled); v1 user/watch |
| `confluence templates get` | 📋 | #13 (unscheduled); v1 templates |
| `confluence templates list` | 📋 | #13 (unscheduled); v1 templates |
| `confluence tree` | 🟡 | missing `--type`; `--target` and `[ref]` take page IDs or URLs only, space targets are #30 |
| `confluence content children` | ➕ | direct children of a page |
| `confluence content delete` | ➕ | twg 1.4.4 splits this into `trash` and `purge` (#30); no confirmation is asked and `--yes` is ignored (#18) |
| `confluence content labels` | ➕ | old form `labels <ref>` of `confluence content labels list --id <ref>` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |

## Bitbucket

78 twg commands: ✅ 12, 🟡 6, 📋 60.

| twg command | atlx | Notes |
| --- | --- | --- |
| `bitbucket branch create` | 📋 | #33 |
| `bitbucket branch delete` | 📋 | #33 |
| `bitbucket branch query` | ✅ | #18; `--author` is matched by atlx over up to 1000 branches, since Bitbucket can't filter by author |
| `bitbucket commit create` | 📋 | #33 |
| `bitbucket commit query` | 📋 | #33 |
| `bitbucket commit report create` | 📋 | #33 |
| `bitbucket commit report query` | 📋 | #33 |
| `bitbucket deployment commits query` | 📋 | #34 |
| `bitbucket deployment environment variable create` | 📋 | #34 |
| `bitbucket deployment environment variable delete` | 📋 | #34 |
| `bitbucket deployment environment variable query` | 📋 | #34 |
| `bitbucket deployment environment variable update` | 📋 | #34 |
| `bitbucket deployment environments query` | 📋 | #34 |
| `bitbucket deployment get` | 📋 | #34 |
| `bitbucket deployment history get` | 📋 | #34 |
| `bitbucket deployment history query` | 📋 | #34 |
| `bitbucket deployment query` | 📋 | #34 |
| `bitbucket inbox` | 📋 | #32 |
| `bitbucket pipeline config get` | 📋 | #34 |
| `bitbucket pipeline config update` | 📋 | #34 |
| `bitbucket pipeline get` | 🟡 | missing `--case-limit`, `--test-cases`, `--test-reports` (#34) |
| `bitbucket pipeline grep` | 📋 | #34 |
| `bitbucket pipeline latest-failure` | 📋 | #34 |
| `bitbucket pipeline query` | 🟡 | missing `--pattern` (#34) |
| `bitbucket pipeline rerun-failed` | 📋 | #34 |
| `bitbucket pipeline run` | 📋 | #34 |
| `bitbucket pipeline tail` | 📋 | #34 |
| `bitbucket pipeline trigger` | 📋 | #34 |
| `bitbucket pull-requests activity` | ✅ | takes several ids as a batch, with `--strict` and `--batch-concurrency` (#18) |
| `bitbucket pull-requests approve` | ✅ |  |
| `bitbucket pull-requests comment create` | 🟡 | missing `--end-from-line`, `--end-line`, `--from-line`, `--image`, `--image-name`, `--pending`, `--start-from-line`, `--start-line` (#31) |
| `bitbucket pull-requests comment delete` | 📋 | #31 |
| `bitbucket pull-requests comment get` | 📋 | #31 |
| `bitbucket pull-requests comment query` | 📋 | #31 |
| `bitbucket pull-requests comment reopen` | 📋 | #31 |
| `bitbucket pull-requests comment resolve` | 📋 | #31 |
| `bitbucket pull-requests comment update` | 📋 | #31 |
| `bitbucket pull-requests commits` | 📋 | #31 |
| `bitbucket pull-requests conflicts` | 📋 | #32 |
| `bitbucket pull-requests create` | 🟡 | missing `--image`, `--image-name`, `--reviewer` (#32) |
| `bitbucket pull-requests decline` | ✅ | no confirmation is asked; `--yes` is ignored (#18) |
| `bitbucket pull-requests default-reviewer add` | 📋 | #32 |
| `bitbucket pull-requests default-reviewer get` | 📋 | #32 |
| `bitbucket pull-requests default-reviewer query` | 📋 | #32 |
| `bitbucket pull-requests default-reviewer remove` | 📋 | #32 |
| `bitbucket pull-requests diff` | ✅ |  |
| `bitbucket pull-requests diff-line` | 📋 | #31 |
| `bitbucket pull-requests diffstat` | 📋 | #31 |
| `bitbucket pull-requests effective-default-reviewer query` | 📋 | #32 |
| `bitbucket pull-requests for-commit` | 📋 | #32 |
| `bitbucket pull-requests get` | ✅ | `--batch-concurrency` fetches in parallel and keeps input order (#18) |
| `bitbucket pull-requests merge` | 🟡 | missing `--timeout`, `--wait`, `--wait-timeout` (#32); no confirmation is asked. The old positional id, `--strategy`, `--message` and `--yes` still work (hidden, deprecated) |
| `bitbucket pull-requests merge-status` | 📋 | #32 |
| `bitbucket pull-requests mergeability` | 📋 | #32 |
| `bitbucket pull-requests patch` | 📋 | #31 |
| `bitbucket pull-requests query` | 🟡 | missing `--merged-before`, `--merged-since`, `--page`, `--reviewer`, `--role` (#32); GraphQL-only `--scope` not planned |
| `bitbucket pull-requests remove-request-changes` | 📋 | #32 |
| `bitbucket pull-requests request-changes` | ✅ |  |
| `bitbucket pull-requests task create` | 📋 | #31 |
| `bitbucket pull-requests task delete` | 📋 | #31 |
| `bitbucket pull-requests task get` | 📋 | #31 |
| `bitbucket pull-requests task query` | ✅ | #18 |
| `bitbucket pull-requests task reopen` | 📋 | #31 |
| `bitbucket pull-requests task resolve` | 📋 | #31 |
| `bitbucket pull-requests task update` | 📋 | #31 |
| `bitbucket pull-requests unapprove` | ✅ |  |
| `bitbucket pull-requests update` | 📋 | #32 |
| `bitbucket repo contributors` | 📋 | #33 |
| `bitbucket repo create` | 📋 | #33 |
| `bitbucket repo file` | ✅ |  |
| `bitbucket repo get` | ✅ |  |
| `bitbucket repo list-files` | 📋 | #33 |
| `bitbucket repo query` | ✅ | `--repo workspace/slug` or a URL selects the workspace (#18) |
| `bitbucket repo url` | 📋 | #33 |
| `bitbucket search prs` | 📋 | #32 |
| `bitbucket workspace get` | 📋 | #33 |
| `bitbucket workspace member get` | 📋 | #33 |
| `bitbucket workspace member query` | 📋 | #33 |
| `bitbucket pull-requests tasks` | ➕ | old name of `bb pull-requests task query` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `bitbucket repo branches` | ➕ | old name of `bb branch query` (#18); hidden; prints a deprecation note in text mode on a terminal; removed in 0.3 |
| `bitbucket search code` | ➕ | Bitbucket code search; twg's `search-code` uses Code Intelligence |

## Users

7 twg commands: ✅ 1, 📋 2, ⛔ 4.

| twg command | atlx | Notes |
| --- | --- | --- |
| `user bulk-lookup` | 📋 | #13 (unscheduled); REST v3 user/bulk |
| `user direct-reports` | ⛔ | reporting lines come from the Teamwork Graph |
| `user get` | 📋 | #13 (unscheduled); REST v3 user / myself |
| `user manager` | ⛔ | reporting lines come from the Teamwork Graph |
| `user manager-chain` | ⛔ | reporting lines come from the Teamwork Graph |
| `user peers` | ⛔ | reporting lines come from the Teamwork Graph |
| `user search` | ✅ | `--compact` is accepted; the JSON is the same either way, as in twg (#21). `--name`, `--query` and `--email` all use Jira's user search, so `sources` is always `["jira-rest"]` (#18); GraphQL-only `--include-slack` not planned |

## Accounts and setup

15 twg commands: ✅ 10, ⛔ 5.

| twg command | atlx | Notes |
| --- | --- | --- |
| `auth profile default` | ✅ |  |
| `auth profile list` | ✅ |  |
| `auth profile migrate` | ✅ |  |
| `auth profile rename` | ✅ |  |
| `auth refresh` | ⛔ | refreshes twg OAuth tokens; atlx uses API tokens |
| `auth storage migrate` | ⛔ | manages twg's encrypted token store; atlx keeps tokens in its config |
| `auth storage reset` | ⛔ | manages twg's encrypted token store; atlx keeps tokens in its config |
| `auth storage rotate-key` | ⛔ | manages twg's encrypted token store; atlx keeps tokens in its config |
| `auth storage status` | ⛔ | manages twg's encrypted token store; atlx keeps tokens in its config |
| `doctor` | ✅ |  |
| `login` | ✅ | `--env` selects a twg environment and does not apply; atlx adds `--token`, `--token-cmd` |
| `logout` | ✅ |  |
| `setup bitbucket` | ✅ |  |
| `setup default-site` | ✅ |  |
| `whoami` | ✅ |  |

## Jira Service Management

121 twg commands: 📋 65, ⛔ 56.

| twg command | atlx | Notes |
| --- | --- | --- |
| `jsm agents availability get` | ⛔ | agent availability is only in the GraphQL gateway |
| `jsm agents availability list` | ⛔ | agent availability is only in the GraphQL gateway |
| `jsm agents availability set` | ⛔ | agent availability is only in the GraphQL gateway |
| `jsm agents list` | ⛔ | agent availability is only in the GraphQL gateway |
| `jsm alert acknowledge` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert assign` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert close` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert create` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert delete` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert escalate` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert get` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert note create` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert note query` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert query` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert request get` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert snooze` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert unacknowledge` | 📋 | #37; JSM Ops alerts REST API |
| `jsm alert update` | 📋 | #37; JSM Ops alerts REST API |
| `jsm approval approve` | 📋 | #37; servicedeskapi approvals |
| `jsm approval get` | 📋 | #37; servicedeskapi approvals |
| `jsm approval list` | 📋 | #37; servicedeskapi approvals |
| `jsm approval reject` | 📋 | #37; servicedeskapi approvals |
| `jsm conversation claim create` | ⛔ | virtual agent conversations (GraphQL gateway) |
| `jsm conversation close create` | ⛔ | virtual agent conversations (GraphQL gateway) |
| `jsm conversation message query` | ⛔ | virtual agent conversations (GraphQL gateway) |
| `jsm conversation query` | ⛔ | virtual agent conversations (GraphQL gateway) |
| `jsm conversation settings query` | ⛔ | virtual agent conversations (GraphQL gateway) |
| `jsm conversation settings update` | ⛔ | virtual agent conversations (GraphQL gateway) |
| `jsm conversation workspace create` | ⛔ | virtual agent conversations (GraphQL gateway) |
| `jsm conversation workspace query` | ⛔ | virtual agent conversations (GraphQL gateway) |
| `jsm form answers list` | 📋 | #37; Jira Forms REST API |
| `jsm form attach` | 📋 | #37; Jira Forms REST API |
| `jsm form describe` | 📋 | #37; Jira Forms REST API |
| `jsm form fields list` | 📋 | #37; Jira Forms REST API |
| `jsm form fill` | 📋 | #37; Jira Forms REST API |
| `jsm form get` | 📋 | #37; Jira Forms REST API |
| `jsm form list` | 📋 | #37; Jira Forms REST API |
| `jsm form reopen` | 📋 | #37; Jira Forms REST API |
| `jsm form submit` | 📋 | #37; Jira Forms REST API |
| `jsm help-article query` | ⛔ | help center articles (GraphQL gateway) |
| `jsm help-center config get` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center create` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center customer-experience list` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center get` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center get-by-project` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center hub-media-config get` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center layout-translations list` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center list` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center list-basic` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center list-by-project` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center media-config get` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center page get` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center page list` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center permission-settings get` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center permissions get` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center product-entities query` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center reporting get` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-center topic get` | ⛔ | help centers are only in the GraphQL gateway |
| `jsm help-object-store create` | ⛔ | help center object store (GraphQL gateway) |
| `jsm help-object-store query` | ⛔ | help center object store (GraphQL gateway) |
| `jsm incident affected-service query` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident affected-services query` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident alert query` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident create` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident delete` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident get` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident link affected-service` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident link alert` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident query` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident responder add` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident responder query` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident responder remove` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident transition` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident unlink affected-service` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident unlink alert` | 📋 | #37; JSM Ops and Jira REST |
| `jsm incident update` | 📋 | #37; JSM Ops and Jira REST |
| `jsm knowledge-app-link query` | ⛔ | knowledge connectors (GraphQL gateway) |
| `jsm knowledge-article query` | 📋 | #37; servicedeskapi knowledge base articles |
| `jsm knowledge-base create` | ⛔ | knowledge configuration (GraphQL gateway) |
| `jsm knowledge-base query` | ⛔ | knowledge configuration (GraphQL gateway) |
| `jsm knowledge-base search query` | ⛔ | knowledge configuration (GraphQL gateway) |
| `jsm knowledge-capability query` | ⛔ | knowledge configuration (GraphQL gateway) |
| `jsm knowledge-discovery create` | ⛔ | Rovo knowledge discovery |
| `jsm knowledge-discovery query` | ⛔ | Rovo knowledge discovery |
| `jsm knowledge-permission bulk query` | ⛔ | knowledge connectors (GraphQL gateway) |
| `jsm knowledge-source-type get` | ⛔ | knowledge connectors (GraphQL gateway) |
| `jsm linked-source link` | ⛔ | knowledge connectors (GraphQL gateway) |
| `jsm linked-source permission update` | ⛔ | knowledge connectors (GraphQL gateway) |
| `jsm linked-source query` | ⛔ | knowledge connectors (GraphQL gateway) |
| `jsm linked-source suggestion query` | ⛔ | knowledge connectors (GraphQL gateway) |
| `jsm linked-source unlink` | ⛔ | knowledge connectors (GraphQL gateway) |
| `jsm linked-source view update` | ⛔ | knowledge connectors (GraphQL gateway) |
| `jsm portal query` | 📋 | #37; servicedeskapi service desks |
| `jsm post-incident-review create` | 📋 | #37; PIRs are Jira work items |
| `jsm post-incident-review delete` | 📋 | #37; PIRs are Jira work items |
| `jsm post-incident-review get` | 📋 | #37; PIRs are Jira work items |
| `jsm post-incident-review incident get` | 📋 | #37; PIRs are Jira work items |
| `jsm post-incident-review link incident` | 📋 | #37; PIRs are Jira work items |
| `jsm post-incident-review query` | 📋 | #37; PIRs are Jira work items |
| `jsm post-incident-review unlink incident` | 📋 | #37; PIRs are Jira work items |
| `jsm post-incident-review update` | 📋 | #37; PIRs are Jira work items |
| `jsm request comment create` | 📋 | #37; servicedeskapi requests |
| `jsm request comment list` | 📋 | #37; servicedeskapi requests |
| `jsm request create` | 📋 | #37; servicedeskapi requests |
| `jsm request participant add` | 📋 | #37; servicedeskapi requests |
| `jsm request participant list` | 📋 | #37; servicedeskapi requests |
| `jsm request participant remove` | 📋 | #37; servicedeskapi requests |
| `jsm request validate` | 📋 | #37; servicedeskapi requests |
| `jsm request-type field get` | 📋 | #37; servicedeskapi request types |
| `jsm request-type fields get` | 📋 | #37; servicedeskapi request types |
| `jsm request-type form get` | 📋 | #37; servicedeskapi request types |
| `jsm request-type query` | 📋 | #37; servicedeskapi request types |
| `jsm resolution-state get` | ⛔ | resolution states (GraphQL gateway) |
| `jsm service get` | ⛔ | service registry (GraphQL gateway) |
| `jsm service query` | ⛔ | service registry (GraphQL gateway) |
| `jsm service search` | ⛔ | service registry (GraphQL gateway) |
| `jsm service-tier query` | ⛔ | service registry (GraphQL gateway) |
| `jsm sla get` | 📋 | #37; servicedeskapi request SLA |
| `jsm sla metrics` | ⛔ | SLA reporting is only in the GraphQL gateway |
| `jsm sla workitems` | ⛔ | SLA reporting is only in the GraphQL gateway |
| `jsm support-site-article query` | ⛔ | help center articles (GraphQL gateway) |

## atlx extras

| twg command | atlx | Notes |
| --- | --- | --- |
| `api` | ➕ | raw REST call |
| `completion script` | ➕ | shell completions |
| `man` | ➕ | man page |
| `search` | ➕ | Jira, Confluence and Bitbucket in one call |
