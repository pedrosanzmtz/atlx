#!/usr/bin/env bash
# Side-by-side output check: run the same read-only commands through twg and
# atlx and compare the shapes of their JSON output.
#
# Needs both CLIs logged in to the same site, plus jq. Every command here is
# read-only. Outputs go to target/conformance/ (git-ignored): they contain
# real site data, so never commit them. Commit only synthetic fixtures.
#
# Usage:
#   scripts/conformance.sh                  # Jira and user commands
#   KEY=PROJ-1 SPACE=PROJ scripts/conformance.sh
#
# Settings (environment):
#   TWG, ATLX    binaries (default: twg and atlx on PATH; ATLX may be a path
#                to target/debug/atlx)
#   KEY, KEY2    work items for get/transitions (default: first two from SPACE)
#   SPACE        Jira project key (default: the first one twg lists)
#   BOARD        board id for `board sprints query` (default: first board)
#   SPRINT       sprint id for `sprint workitems query` (skipped when unset
#                and the board has no sprints)
#   USER_QUERY   name for `user search` (default: the logged-in user's name)
#   CONF_PAGE    Confluence page id: enables the Confluence commands
#   CONF_SPACE   Confluence space key for `content list` and `space get`
#   BB_REPO      workspace/slug: enables the Bitbucket commands
#   BB_PR        pull request id in BB_REPO (default: the newest one)
#
# The report (target/conformance/report.txt) lists, per command, the JSON
# paths only one side prints and the paths whose value types differ. Array
# indexes are folded to [], so it compares structure, not values.

set -uo pipefail

TWG=${TWG:-twg}
ATLX=${ATLX:-atlx}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${OUT:-$ROOT/target/conformance}
mkdir -p "$OUT"
REPORT=$OUT/report.txt
: >"$REPORT"

command -v jq >/dev/null || {
  echo "conformance: jq is required" >&2
  exit 1
}

log() { printf '%s\n' "$*" | tee -a "$REPORT"; }

# Every leaf path of a JSON document with its type, array indexes folded.
shape() {
  jq -r '
    [paths as $p
     | {p: ($p | map(if type == "number" then "[]" else . end) | join(".")),
        t: (getpath($p) | type)}]
    | map("\(.p)\t\(.t)") | unique | .[]' "$1" 2>/dev/null
}

# run NAME ARGS...: run the same arguments through both CLIs and compare.
run() {
  local name=$1
  shift
  local t=$OUT/$name.twg.json a=$OUT/$name.atlx.json
  "$TWG" "$@" -o json >"$t" 2>"$OUT/$name.twg.err"
  "$ATLX" "$@" -o json >"$a" 2>"$OUT/$name.atlx.err"
  compare "$name" "$t" "$a"
}

compare() {
  local name=$1 t=$2 a=$3
  if ! jq -e . "$t" >/dev/null 2>&1 || ! jq -e . "$a" >/dev/null 2>&1; then
    log "== $name: no JSON from $(jq -e . "$t" >/dev/null 2>&1 || echo -n 'twg ')$(jq -e . "$a" >/dev/null 2>&1 || echo -n 'atlx')"
    return
  fi
  if jq -e '.ok == false' "$t" >/dev/null 2>&1; then
    log "== $name: twg returned an error ($(jq -r .error.code "$t")); skipped"
    return
  fi
  # Values twg echoes from its OAuth and Teamwork Graph setup, which atlx
  # can't reproduce; they are compared for presence only, not shape.
  local ignore='^(meta\.|request\.)'
  local ts as
  ts=$(shape "$t" | grep -Ev "$ignore")
  as=$(shape "$a" | grep -Ev "$ignore")
  local only_t only_a types
  only_t=$(comm -23 <(cut -f1 <<<"$ts" | sort -u) <(cut -f1 <<<"$as" | sort -u))
  only_a=$(comm -13 <(cut -f1 <<<"$ts" | sort -u) <(cut -f1 <<<"$as" | sort -u))
  types=$(join -t $'\t' <(sort <<<"$ts") <(sort <<<"$as") 2>/dev/null |
    awk -F'\t' '$2 != $3 && $2 != "null" && $3 != "null" {print $1 " twg=" $2 " atlx=" $3}')
  if [ -z "$only_t$only_a$types" ]; then
    log "== $name: same shape"
    return
  fi
  log "== $name"
  [ -n "$only_t" ] && log "  only in twg:" && sed 's/^/    /' <<<"$only_t" | tee -a "$REPORT"
  [ -n "$only_a" ] && log "  only in atlx:" && sed 's/^/    /' <<<"$only_a" | tee -a "$REPORT"
  [ -n "$types" ] && log "  type differs:" && sed 's/^/    /' <<<"$types" | tee -a "$REPORT"
  return 0
}

# --- discover defaults (read-only) -------------------------------------------

SPACE=${SPACE:-$("$TWG" jira space query -o json 2>/dev/null | jq -r '.data.projects[0].key // empty')}
if [ -n "$SPACE" ]; then
  keys=$("$TWG" jira workitem query --jql "project = $SPACE ORDER BY key" --limit 2 -o json 2>/dev/null |
    jq -r '.data.issues[].key')
  KEY=${KEY:-$(sed -n 1p <<<"$keys")}
  KEY2=${KEY2:-$(sed -n 2p <<<"$keys")}
fi
BOARD=${BOARD:-$("$TWG" jira board query -o json 2>/dev/null | jq -r '.data.boards[0].id // empty')}
USER_QUERY=${USER_QUERY:-$("$TWG" whoami -o json 2>/dev/null | jq -r '.data.name // empty')}

log "twg $("$TWG" --version 2>/dev/null) vs $("$ATLX" --version 2>/dev/null)"
log "space=${SPACE:-?} key=${KEY:-?} key2=${KEY2:-?} board=${BOARD:-?}"

# --- Jira and users ----------------------------------------------------------

run whoami whoami
if [ -n "$SPACE" ]; then
  run space-query jira space query
  run space-get jira space get "$SPACE"
  run workitem-query jira workitem query --jql "project = $SPACE ORDER BY key" --limit 3
  run workitem-query-page jira workitem query --jql "project = $SPACE ORDER BY key" --limit 1
  run workitem-query-fields jira workitem query --jql "project = $SPACE" --limit 2 --fields summary,assignee,labels
  run workitem-search jira workitem search "$SPACE" --limit 3
fi
if [ -n "${KEY:-}" ]; then
  run workitem-get jira workitem get "$KEY"
  run workitem-get-comments jira workitem get "$KEY" --comments
  run workitem-get-fields jira workitem get "$KEY" --fields summary,status
  run workitem-get-full jira workitem get "$KEY" --full
  [ -n "${KEY2:-}" ] && run workitem-get-batch jira workitem get "$KEY" "$KEY2"
  run transitions-query jira workitem transitions query --id "$KEY"
  issuetype=$("$TWG" jira workitem get "$KEY" -o json 2>/dev/null | jq -r '.data[0].issuetype.name')
  run create-metadata jira workitem field create-metadata --space "$SPACE" --type "$issuetype"
  run comment-query jira workitem comment query --issue-id "$KEY"
  run comment-query-page jira workitem comment query --issue-id "$KEY" --first 1 --after 1 --order-by=-created
  run changelog-query jira workitem changelog query --issue-id "$KEY"
  run changelog-query-page jira workitem changelog query --issue-id "$KEY" --first 1
  run update-metadata jira workitem field update-metadata --id "$KEY"
  run update-metadata-system jira workitem field update-metadata --id "$KEY" --include-system-fields
  run link-query jira workitem link query --issue-id "$KEY"
  run types-query-issue jira workitem types query --issue "$KEY" --level 0 -n 1
fi
run link-types jira workitem link-types query
[ -n "$SPACE" ] && run types-query jira workitem types query --project "$SPACE"
run statuses-query jira workitem statuses query
run priorities-query jira workitem priorities query
run board-query jira board query
if [ -n "$BOARD" ]; then
  run board-sprints jira board sprints query --board-id "$BOARD"
fi
if [ -n "${SPRINT:-}" ]; then
  run sprint-workitems jira sprint workitems query --sprint-id "$SPRINT"
fi
if [ -n "$USER_QUERY" ]; then
  run user-search user search --name "$USER_QUERY"
  run user-search-compact user search --name "$USER_QUERY" --compact
fi

# --- Confluence (only with CONF_PAGE) ----------------------------------------

if [ -n "${CONF_PAGE:-}" ]; then
  run content-get confluence content get "$CONF_PAGE"
  run content-get-full confluence content get "$CONF_PAGE" --detail full
  run content-get-md confluence content get "$CONF_PAGE" --detail full --format md
  run content-tree confluence tree "$CONF_PAGE"
  run search-text confluence search text "${CONF_TEXT:-the}" --limit 3
  run search-query confluence search query --cql "type = page" --limit 3
  run space-list confluence space list --limit 3
  if [ -n "${CONF_SPACE:-}" ]; then
    run content-list confluence content list --space "$CONF_SPACE" --limit 3
    run space-get-conf confluence space get "$CONF_SPACE"
  fi
fi

# --- Bitbucket (only with BB_REPO) -------------------------------------------

if [ -n "${BB_REPO:-}" ]; then
  ws=${BB_REPO%%/*}
  run repo-query bitbucket repo query --workspace "$ws"
  run repo-get bitbucket repo get --repo "$BB_REPO"
  run branch-query bitbucket branch query --repo "$BB_REPO"
  run pr-query bitbucket pull-requests query --repo "$BB_REPO" --state MERGED
  BB_PR=${BB_PR:-$("$TWG" bitbucket pull-requests query --repo "$BB_REPO" --state MERGED -o json 2>/dev/null |
    jq -r '(.values // .data.values // .)[0].id // empty')}
  if [ -n "$BB_PR" ]; then
    run pr-get bitbucket pull-requests get "$BB_PR" --repo "$BB_REPO"
    run pr-activity bitbucket pull-requests activity "$BB_PR" --repo "$BB_REPO"
    run pr-tasks bitbucket pull-requests task query --pull-request "$BB_PR" --repo "$BB_REPO"
  fi
  run pipeline-query bitbucket pipeline query --repo "$BB_REPO"
fi

log ""
log "Outputs and this report are in $OUT (git-ignored; do not commit them)."
