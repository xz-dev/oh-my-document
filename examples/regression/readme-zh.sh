#!/usr/bin/env bash
# Historical automated regression, not the interactive tutorial.
set -euo pipefail
OMD_BIN=${OMD_BIN:-omd}
DEMO=$(mktemp -d)
cleanup() {
  if [ "${KEEP_DEMO:-0}" = 1 ]; then
    printf 'demo_dir=%s\n' "$DEMO"
  else
    rm -rf "$DEMO"
  fi
}
trap cleanup EXIT
export HOME="$DEMO/home"
export OMD_CONFIG_PATH="$DEMO/config"
export OMD_CACHE_PATH="$DEMO/cache"
mkdir -p "$HOME" "$OMD_CONFIG_PATH" "$OMD_CACHE_PATH" "$DEMO/project"
cd "$DEMO/project"

json_field() {
  python3 -c 'import json,sys
v=json.load(sys.stdin)
for key in sys.argv[1].split("."):
    v=v[int(key)] if isinstance(v,list) else v[key]
print(json.dumps(v,separators=(",",":")) if isinstance(v,(dict,list)) else v)' "$1"
}
observe() {
  local name=$1
  "$OMD_BIN" verify --json > "$DEMO/observation-$name.json"
  json_field data.expected < "$DEMO/observation-$name.json" > "$DEMO/expected-$name.json"
}

printf '最多重试 3 次。\n' > 需求.md
printf 'MAX_RETRIES = 3\n' > retry.py

"$OMD_BIN" init 需求.md --json > "$DEMO/init-spec.json"
observe init-code
"$OMD_BIN" init retry.py --expected "$DEMO/expected-init-code.json" --json \
  > "$DEMO/init-code.json"

observe spec-range
"$OMD_BIN" commit commit 需求.md --range 0 9 --mode text \
  --reason '约定最多重试 3 次' \
  --expected "$DEMO/expected-spec-range.json" --json > "$DEMO/spec-range.json"
SPEC_RANGE=$(json_field data.object.chain_root_commit_id < "$DEMO/spec-range.json")

observe code-range
"$OMD_BIN" commit commit retry.py --range 0 15 --mode text \
  --link-from "$SPEC_RANGE" \
  --reason '用 MAX_RETRIES 实现重试上限' \
  --expected "$DEMO/expected-code-range.json" --json > "$DEMO/code-range.json"
CODE_RANGE=$(json_field data.object.chain_root_commit_id < "$DEMO/code-range.json")
LINK_ID=$(json_field data.link_records.0.link_id < "$DEMO/code-range.json")

# OMD 记录逻辑改名；工作区文件需要另行移动。
observe rename
"$OMD_BIN" rename retry.py retry_limit.py \
  --expected "$DEMO/expected-rename.json" --json > "$DEMO/rename.json"
mv retry.py retry_limit.py

"$OMD_BIN" list --json > "$DEMO/list.json"
"$OMD_BIN" log "$CODE_RANGE" --json > "$DEMO/log.json"
python3 - "$DEMO/list.json" "$SPEC_RANGE" "$CODE_RANGE" "$LINK_ID" <<'PY'
import json,sys
v=json.load(open(sys.argv[1]))
spec,code,link=sys.argv[2:]
objects=v["data"]["objects"]
assert any(o["chain_root_commit_id"]==spec for o in objects)
assert any(o["chain_root_commit_id"]==code and
           o["project_relative_path"]=="retry_limit.py" for o in objects)
assert any(x["link_id"]==link for x in v["data"]["links"])
PY

printf '最多重试 5 次。\n' > 需求.md
set +e
"$OMD_BIN" verify --json > "$DEMO/verify-dirty.json"
VERIFY_STATUS=$?
set -e
test "$VERIFY_STATUS" -eq 1
python3 - "$DEMO/verify-dirty.json" "$SPEC_RANGE" <<'PY'
import json,sys
v=json.load(open(sys.argv[1]))
assert v["data"]["ok"] is False
assert "range:"+sys.argv[2] in v["data"]["dirty"]
PY

set +e
"$OMD_BIN" check --json > "$DEMO/check.json"
CHECK_STATUS=$?
set -e
test "$CHECK_STATUS" -eq 1
printf 'spec_range=%s\ncode_range=%s\nlink_id=%s\n' \
  "$SPEC_RANGE" "$CODE_RANGE" "$LINK_ID"
