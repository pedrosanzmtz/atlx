# Changelog

All notable changes to atlx are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The release workflow copies the section whose heading matches the tag into the
GitHub release notes, so keep one `## [X.Y.Z] - YYYY-MM-DD` heading per release.

## [Unreleased]

### Added

- Jira triage commands with twg's names and output (#23): `jira workitem
  comment query|update|delete`, `comment create --parent-id`, `changelog
  query`, `field update-metadata`, `link query`, `unlink workitem|weblink`,
  `delete`, `types query|get`, `statuses query` and `priorities query`.
  Comment and changelog pages print `pageInfo` with the next `--after`
  cursor. Deletes ask for no confirmation, as in twg.

- Confluence section reads and targeted edits (#27): `content get --detail
  outline` (headings with level, anchor, ADF `localId` and word count),
  `--section <heading>` (repeatable; Markdown by default, the exact storage
  slice with `--format html`), the atlx-extra `--format adf`, and
  `content update --edits`/`--edits-file` with twg's `localId` operations
  plus heading- and text-based extras. Edits apply all-or-nothing to the ADF
  body, honour `--snapshot-token`, and `--dry-run` prints the resulting HTML
  and a unified diff without writing.

- twg's global output controls: `--select` (dot-path projection with
  `@compact`, `@evidence` and `@rows` presets), `--output-file`,
  `--output-summary [none|stats|auto|inline]` with its YAML envelope,
  `--agent-fields`, and `--api-version v2`. `confluence content get
  --body-only --output-file` keeps the snapshot token on stdout (#19).

- `atlx completion script <bash|zsh|fish|powershell|elvish>` prints a shell
  completion script, using the same command shape as twg (#35).
- Man pages generated from the command tree, shipped in release archives (#35).
- Release workflow: tagged cross-platform builds for Linux (gnu and static
  musl, x86_64 and aarch64), macOS (x86_64 and Apple silicon) and Windows
  (x86_64 and ARM64), named to match the `cargo binstall` metadata, with
  `SHA256SUMS`, release notes from this file and a crates.io publish (#35).
- HTTP client transport: binary downloads, multipart uploads, per-request
  headers, retries on 429 and 5xx with backoff, and response bodies larger
  than 10 MB (#16).
- `CONTRIBUTING.md` and `AGENTS.md` contributor guides (#14).

- Typed errors: `client::ApiError` carries the error class, HTTP status,
  request and hint, and `client::Bytes` reports its HTTP status (#17).
- Named profiles: global `-p/--profile` and `ATLX_PROFILE`, `login
  --new-profile`, per-profile `setup bitbucket` and `setup default-site`, and
  `auth profile list|default|rename|migrate`. v1 config files load unchanged as
  the `default` profile (#20).
- Credential helpers: `token_cmd` / `bitbucket.token_cmd`, `ATLX_TOKEN_CMD`,
  `ATLX_BB_TOKEN_CMD` and `--token-cmd` on `login` and `setup bitbucket`,
  run lazily with a 10 s timeout. `whoami` and `doctor` report the active
  profile and token source (#20).

- twg 1.4.4 command names and flags for commands that had drifted (#18):
  `jira space query|list|get`, `jira board query`, `jira board sprints query`,
  `jira sprint workitems query`, `jira workitem field create-metadata` (also
  `custom-fields`), `jira workitem link-types query`, `jira workitem link
  workitem` (repeatable `--target-id`) and `link weblink` (with `--global-id`,
  `--application-name/-type`, `--summary`, `--icon-url`, `--resolved`),
  `confluence content labels list --id`, `comments list|create --id` with
  `--comment-type` and `--body-format` (create also takes `--body-file`),
  `confluence tree --target`, `confluence space get|list --labels`,
  `bb branch query` (`--author`, `--sort`), `bb pull-requests task query`,
  `merge --pull-request --merge-strategy --merge-message
  --keep-source-branch`, batched `bb pull-requests activity <id...>` with
  `--strict`, `bb repo query --repo`, the `-w`/`-r` short flags, and `user
  search --name --query --email --account-id -n`.
- `--batch-concurrency <1-20>` (default 5) on `jira workitem get`, `bb
  pull-requests get` and `bb pull-requests activity`: batch reads now run in
  parallel on scoped threads and keep the input order (#18).

- `scripts/conformance.sh` compares twg's and atlx's JSON on a live site
  (read-only; outputs stay in the git-ignored `target/conformance/`), and
  `scripts/twg-fixtures.sh` turns a run into the synthetic fixtures behind
  `tests/twg_shapes.rs` (#21).
- twg commands that need Teamwork Graph, Rovo or the GraphQL gateway (`rovo`,
  `work`, `context`, `jira board cells query`, ...) fail with "not supported
  by atlx (requires Teamwork Graph)" in twg's error envelope and exit code 2.
  Planned twg commands fail with a pointer to their issue (#21).
- `user search --compact` (#21).

### Changed

- **JSON output now matches twg** (#21). Jira, Confluence, user and account
  commands print twg's envelope (`apiVersion`, `command`, `request`, `data`,
  `pageInfo`, `meta`) with twg's key names, which are mostly the REST APIs'
  own: `jira workitem get` prints `data: [<issue>]` with REST fields next to
  `key`, `query`/`search` print `data.issues` rows and `pageInfo.nextCursor`
  instead of `next_cursor`, `transitions query` prints `{id, name, toName,
  category}`, `space query` prints `projects`, `board query` `boards`, and
  `whoami` twg's profile fields. Bitbucket commands print Bitbucket's REST
  objects without an envelope, as twg does. Batches gain `returned`,
  `omitted` and `truncated` counts. `-o jsonl` adds twg's `_twg` locator to
  each row. The old snake_case summary keys are gone, and `--select` presets
  no longer carry fallbacks for them.
- `user search` returns 20 users by default, as twg does (was 25).
- `jira space query` sorts by key unless `--sort-by` is given, as twg does.
- Merge, decline and `confluence content delete` no longer require `--yes`,
  matching twg; `--yes` is still accepted and ignored (#18).
- The 0.1 names (`jira project`, `jira board list`, `jira sprint list|issues`,
  `jira workitem create-meta`, `jira workitem link types|create|remote`, `bb
  repo branches`, `bb pull-requests tasks`, positional `labels`/`comments`/
  `merge` forms, and `--inline`, `--format`, `--strategy`, `--message`) still
  work but are hidden from help. In text mode on a terminal they print a
  one-line note naming the new form; they will be removed in 0.3 (#18).
- `atlx logout` removes only the selected profile (the whole file only when
  nothing else is left), as twg does (#20).
- `login` and `setup bitbucket` accept `-f` for `--force`, and `setup bb` is an
  alias of `setup bitbucket`, matching twg (#20).

- Errors follow twg: in JSON modes an `{"ok": false, "error": {...}}` envelope
  goes to stdout and a readable report to stderr. Every failure exits 1
  (clap argument errors included, previously 2), and a partly failed batch
  read exits 3. Batch items now carry structured errors plus a `summary`, and
  `atlx search` reports per-product errors as objects (#17).
- Rich-text bodies convert with better fidelity: Markdown and HTML to Jira ADF
  and Confluence storage, and ADF back to GitHub-flavoured Markdown (#22).
- The CLI moved into `src/cli/` and the mock-HTTP tests were split per product,
  with a snapshot test of the whole command tree. No behaviour change (#14).

## [0.1.0] - 2026-11-13

First release.

### Added

- twg-compatible commands for Jira (`jira workitem get/search/query/create/
  update/comment create/transition/transitions query`), Confluence (`confluence
  search query/text`, `content get/create/update/list`, `space get`) and
  Bitbucket (`bb repo`, `bb pull-requests`, `bb pipeline`), on the public REST
  APIs with API-token auth.
- `login`, `logout`, `whoami` and `setup bitbucket/default-site`.
- Cross-product `atlx search` and a raw `atlx api` escape hatch.
- Output as text on a terminal and JSON when piped, plus `-o jsonl`.
- Jira metadata and Agile commands: `workitem create-meta`, issue links and
  remote links, `field list`, `project list/get`, `board list`, `sprint
  list/issues`, and `user search` (#3).
- Confluence comments (including inline), page tree, personal space, lookup by
  title, labels and delete (#4).
- Bitbucket PR diff, activity and tasks; approve, request changes, merge and
  decline; repository files and branches; code search (#5).
- `atlx doctor` for config, permission and connectivity checks; GitHub Actions
  CI on Linux, macOS and Windows; `cargo binstall` metadata; twg compatibility
  notes in the README (#6).
- Offline mock-HTTP integration tests and opt-in live smoke tests (#2).

### Fixed

- Argument validation, pagination and auth-origin edge cases (#1).

[Unreleased]: https://github.com/pedrosanzmtz/atlx/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/pedrosanzmtz/atlx/releases/tag/v0.1.0
