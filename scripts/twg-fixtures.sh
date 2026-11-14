#!/usr/bin/env bash
# Turn a conformance run into synthetic test fixtures for tests/twg_shapes.rs.
#
# Run scripts/conformance.sh first. This script then
#   1. reads twg's outputs from target/conformance/<name>.twg.json, and
#   2. fetches the REST responses atlx needs for the same commands
#      (read-only GETs through `atlx api`),
# and writes both to tests/fixtures/twg/ with every value replaced by a
# placeholder of the same type ("text", 1, true), and arrays cut to one
# element. Only key names and types survive, so no site data is committed.
# Review the diff before committing anyway.
#
# Settings: ATLX (binary), KEY (work item, default KAN-1), SPACE (default:
# KEY's project), ISSUE_TYPE (default Task), USER_QUERY (a name).

set -euo pipefail

ATLX=${ATLX:-atlx}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
IN=$ROOT/target/conformance
OUT=$ROOT/tests/fixtures/twg
KEY=${KEY:-KAN-1}
SPACE=${SPACE:-${KEY%%-*}}
ISSUE_TYPE=${ISSUE_TYPE:-Task}
mkdir -p "$OUT/rest"

# Same shape, no data: strings, numbers and booleans become placeholders.
# Custom field ids, twg's envelope labels (command, resourceType, ...) and
# field operations (set, add, ...) are kept: they name things, not data.
# Object keys are field names; `self` and other URLs are values and get
# replaced.
SANITIZE='
  def scrub($k):
    if type == "object" then with_entries(.key as $kk | .value |= scrub($kk))
    elif type == "array" and $k == "operations" then .
    elif type == "array" then (if length > 0 then [.[0] | scrub($k)] else [] end)
    elif type == "string" then
      (if (($k == "fieldId" or $k == "key" or $k == "id") and test("^customfield_[0-9]+$"))
          or ($k | IN("apiVersion", "command", "resourceType", "resourceClass", "sourceMode", "operations"))
       then . else "text" end)
    elif type == "number" then 1
    else . end;
  scrub("")'

for name in whoami space-query space-get workitem-query \
  workitem-get workitem-get-batch transitions-query create-metadata link-types \
  board-query user-search comment-query changelog-query update-metadata \
  types-query statuses-query priorities-query; do
  f=$IN/$name.twg.json
  [ -s "$f" ] || {
    echo "skip $name (no $f)"
    continue
  }
  jq "$SANITIZE" "$f" >"$OUT/$name.json"
done

# rest FILE PATH [FILTER]: GET PATH, optionally filter it, sanitize, save.
rest() {
  local file=$1 path=$2 filter=${3:-.}
  "$ATLX" api jira "$path" -o json | jq "$filter" | jq "$SANITIZE" >"$OUT/rest/$file.json"
}
get_fields=summary,issuetype,project,status,statusCategory,priority,assignee,reporter,creator
get_fields=$get_fields,created,updated,duedate,resolution,resolutiondate,labels,components
get_fields=$get_fields,fixVersions,description,subtasks,issuelinks,attachment,comment,worklog
get_fields=$get_fields,timetracking,votes,watches,parent
rest issue "/rest/api/3/issue/$KEY?fields=$get_fields"
rest search "/rest/api/3/search/jql?jql=key%3D$KEY&fields=summary,status,issuetype,priority,assignee,reporter,project,created,updated"
rest transitions "/rest/api/3/issue/$KEY/transitions"
rest project-search "/rest/api/3/project/search?orderBy=key&maxResults=50"
rest project "/rest/api/3/project/$SPACE"
rest board "/rest/agile/1.0/board?maxResults=50"
rest link-types "/rest/api/3/issueLinkType"
type_id=$("$ATLX" api jira "/rest/api/3/issue/createmeta/$SPACE/issuetypes" -o json |
  jq -r --arg t "$ISSUE_TYPE" '(.issueTypes // .values)[] | select(.name == $t) | .id' | head -1)
# Custom fields only, so the one element kept is one twg prints.
rest createmeta "/rest/api/3/issue/createmeta/$SPACE/issuetypes/$type_id" \
  '(.fields // .values) |= map(select(.fieldId | startswith("customfield_")))'
rest myself "/rest/api/3/myself"
rest comments "/rest/api/3/issue/$KEY/comment?startAt=0&maxResults=100"
rest changelog "/rest/api/3/issue/$KEY/changelog?startAt=0&maxResults=100"
# Only the field twg's sanitized output kept (its first), so both sides
# describe the same field.
first_field=$(jq -r '.data.fields[0].id // empty' "$IN/update-metadata.twg.json" 2>/dev/null)
rest editmeta "/rest/api/3/issue/$KEY/editmeta" \
  ".fields |= with_entries(select(.key == \"$first_field\"))"
project_id=$("$ATLX" api jira "/rest/api/3/project/$SPACE" -o json | jq -r .id)
rest issuetype-project "/rest/api/3/issuetype/project?projectId=$project_id"
rest statuses "/rest/api/3/status"
rest priorities "/rest/api/3/priority"
rest user-search "/rest/api/3/user/search?query=${USER_QUERY:-a}&maxResults=1"
echo "Wrote $OUT; review the diff before committing."
