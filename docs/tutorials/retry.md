# Hands-on tutorial: connect a retry requirement to its implementation

[Back to README](../../README.md) · [简体中文](retry.zh-CN.md)

### The requirement changes from 3 retries to 5. Which implementation needs review?

Connect a requirement to its implementation, change the requirement, find the code that still says 3, and record how you handled it. **Read each result before continuing; finishing a script is not the goal.**

You need Bash, `omd`, [jq](https://jqlang.org/), and Python 3. Run the steps in order in the same new Bash session. Stop on unexpected errors. This example uses a constant to express a retry limit, not a complete retry algorithm.

#### 1. Two files together do not yet have a relationship

```bash
DEMO=$(mktemp -d)
export OMD_CONFIG_PATH="$DEMO/config"
export OMD_CACHE_PATH="$DEMO/cache"
mkdir "$DEMO/project"
cd "$DEMO/project"
printf 'Retry at most 3 times.\n' > spec.md
printf 'MAX_RETRIES = 3\n' > retry.py
printf 'Demo directory: %s\n' "$DEMO"
```

`spec.md` states the requirement; `retry.py` expresses the limit as a constant. No tool knows why they belong together yet. Configuration and cache stay inside the demo, separate from existing projects. Files remain afterward; exit this Bash session to restore your previous environment.

#### 2. Register files without claiming their content is reviewed

```bash
omd init spec.md --json > init-spec.json
jq '.data | {ok, commit}' init-spec.json
```

`ok: true` means a file baseline was saved. **Initialization does not confirm content, infer an implementation, or create a link.**

Every subsequent write needs a fresh observation from `verify`, passed back as `--expected`. This prevents you from unknowingly confirming content somebody else just changed. The current CLI requires JSON plumbing; we collect the repetitive part below without hiding what it does.

<details>
<summary>Expand and run once: what observe actually does</summary>

This is a tutorial Bash function, **not an OMD command**. It runs verify once, displays the result, and extracts `data.expected`. It does not commit, confirm, or retry.

```bash
observe() {
  local status=0
  omd verify --json > observation.json || status=$?
  printf 'verify exit=%s\n' "$status"
  if [ "$status" -gt 1 ]; then
    jq . observation.json
    return "$status"
  fi
  jq '.data | {ok, dirty, locate, missing, unverified, obligations, open_blocks}' observation.json
  jq '.diagnostics' observation.json
  jq -e '.data.expected // error("No usable observation; stop")' observation.json > expected.json
}
```

Exit 1 can mean a review is needed after our deliberate edit, not that no observation exists. Read the result first. Stop for missing sources, failed acquisition, lock conflicts, or other unexpected outcomes. Do not refresh credentials and retry automatically, or paste all remaining commands at once.

</details>

```bash
observe
omd init retry.py --expected expected.json --json > init-code.json
jq '.data | {ok, commit}' init-code.json
```

Both initializations should succeed. There are still no reviewed ranges or links.

#### 3. Explain why this code implements that requirement

Select `[0,22)` in the requirement: zero-based, excluding the right endpoint. This selects the 22 characters of “Retry at most 3 times.” without its newline. Text coordinates count Unicode characters, not lines or UTF-8 bytes.

```bash
observe
omd commit commit spec.md --range 0 22 --mode text \
  --reason 'Require at most 3 retries' --expected expected.json --json > spec-range.json
SPEC_ID=$(jq -er '.data.object.chain_root_commit_id' spec-range.json)
jq '.data.object | {chain_root_commit_id, position}' spec-range.json
```

`commit commit` is the actual syntax: the subcommand followed by the ordinary commit kind. `SPEC_ID` comes from your result; do not copy someone else's ID.

Select the 15 characters of `MAX_RETRIES = 3` and add an incoming link:

```bash
observe
omd commit commit retry.py --range 0 15 --mode text \
  --link-from "$SPEC_ID" --reason 'Express the required retry limit with MAX_RETRIES' \
  --expected expected.json --json > code-range.json
CODE_ID=$(jq -er '.data.object.chain_root_commit_id' code-range.json)
CODE_TIP=$(jq -er '.data.object.tip_commit_id' code-range.json)
LINK_ID=$(jq -er '.data.link_records[0].link_id' code-range.json)
omd links --json > linked.json
omd links show "$LINK_ID" --json | jq '.link | {link_id, from: .full.source.object.root_commit_id, to: .full.target.object.root_commit_id}'
```

Check the output: one link goes from `SPEC_ID` to `CODE_ID`, representing:

```text
Requirement: Retry at most 3 times. → Implementation: MAX_RETRIES = 3
```

Range identity answers “which fragment,” link ID identifies the relationship, and tip is the current version for continuation. You supply the reason; a variable named MAX_RETRIES does not prove program correctness.

#### 4. Change the requirement, not the code yet

```bash
printf 'Retry at most 5 times.\n' > spec.md
observe
```

Expect exit 1 and `ok: false`. The `dirty` object contains `range:<SPEC_ID>` with an `in-range edit` reason. **Tracked content changed and needs review; this is not a crash.**

Follow the relationship instead of relying on a remembered filename. The link's full projection resolves each endpoint — the target's `resolved` projection shows its path and range:

```bash
omd links show "$LINK_ID" --json > linked.json
jq '.link.full.target.resolved | {project_relative_path, position}' linked.json
python3 -c 'from pathlib import Path; print(Path("retry.py").read_text(), end="")'
```

The target is `retry.py`, range `[0,15)`, still containing `MAX_RETRIES = 3`. **Your decision now: accept the new limit? Change this implementation?**

#### 5. Accept the requirement and record the implementation's response

Here we accept 5 retries. The only expected issue in the observation is our own requirement edit. Use that evidence to continue the existing range:

```bash
omd commit commit spec.md --id "$SPEC_ID" --range 0 22 --mode text \
  --reason 'Raise the required retry limit from 3 to 5' \
  --expected expected.json --json > spec-five.json
CHANGE_ID=$(jq -er '.data.commit' spec-five.json)
```

This range has not been revised before, so its root is also its current tip. In general, query the latest tip rather than reusing the root. After recording the requirement, the current `verify` may already succeed. **That does not mean the implementation changed.** We saw that it still says 3, so continue the review.

```bash
printf 'MAX_RETRIES = 5\n' > retry.py
python3 -c 'from retry import MAX_RETRIES; print("Current limit:", MAX_RETRIES); assert MAX_RETRIES == 5'
observe
```

The small check prints `Current limit: 5`. It checks the constant only—not a real retry loop, exception handling, or side effects. Production code needs its own relevant tests.

Save the implementation revision first, then record its adaptation separately, naming **the link, the requirement change, and the reason**:

```bash
omd commit commit retry.py --id "$CODE_TIP" --range 0 15 --mode text \
  --reason 'Implement the new retry limit of 5' \
  --expected expected.json --json > code-five.json
jq '.data | {ok, commit}' code-five.json
NEW_CODE_TIP=$(jq -er '.data.object.tip_commit_id' code-five.json)
```

```bash
observe
ADAPT=$(jq -nc --arg link "$LINK_ID" --arg change "$CHANGE_ID" \
  '{link_id:$link, changes:[$change], reason:"Reviewed the new requirement, changed MAX_RETRIES from 3 to 5, and checked its value"}')
omd commit adapt retry.py --id "$NEW_CODE_TIP" --adapt "$ADAPT" \
  --expected expected.json --json > adapted.json
jq '.data | {ok, kind, commit}' adapted.json
```

Look for `kind: "Adapt"`: this is the separate adaptation record. Use explicit `commit adapt`; a successful ordinary `commit commit` with `--adapt` does not currently establish that adaptation was recorded.

`--adapt` is not “make everything green”: it selects this one change on this one link, not other relationships.

#### 6. Inspect the result and keep the limits clear

```bash
observe
REVIEW_TIP=$(jq -er '.data.object.tip_commit_id' adapted.json)
omd log "$REVIEW_TIP" --json > history.json
jq '.data | {chain, selected: (.selected | {chain_root_commit_id, tip_commit_id, position})}' history.json
```

Expect `ok: true`, with empty dirty, locate, missing, and outstanding-review collections. History retains the original range identity and its new version; the full response is in `history.json`. `spec-five.json`, `code-five.json`, and `adapted.json` hold the results, while `.omd/` preserves reasons and adaptation selections.

You followed **connect → detect change → locate implementation → judge and check → record the response**. OMD preserves traceable relationships and decisions; it does not prove business correctness or full project coverage. This example does not configure coverage statistics or rules; see [import in the README](../../README.md#manage-files-and-directories).

Rename, cache handling, and regression assertions do not belong in the first lesson. The former automated scripts remain under [examples/regression](../../examples/regression/README.md) for maintainers, not as the human introduction.
