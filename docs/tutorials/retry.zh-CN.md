# 手动教程：把重试需求连到实现

[返回 README](../../README.zh-CN.md) · [English](retry.md)

### 需求从 3 次改为 5 次，哪段实现要跟着检查？

下面只做这一件事。你会亲手建立“需求 → 实现”的关联，修改需求，找到仍然写着 3 的代码，再记录处理结果。**不是运行完脚本就算学会：每一步先看输出，再决定是否继续。**

需要 Bash、`omd`、[jq](https://jqlang.org/) 和 Python 3。在同一个新 Bash 会话中顺序操作，遇到非预期错误就停止。示例只用常量表达重试上限，不是完整重试算法。

#### 1. 两个文件放在一起，还不等于有关联

```bash
DEMO=$(mktemp -d)
export OMD_CONFIG_PATH="$DEMO/config"
export OMD_CACHE_PATH="$DEMO/cache"
mkdir "$DEMO/project"
cd "$DEMO/project"
printf '最多重试 3 次。\n' > 需求.md
printf 'MAX_RETRIES = 3\n' > retry.py
printf '示例目录：%s\n' "$DEMO"
```

`需求.md` 说“最多重试 3 次”，`retry.py` 用常量表达上限。现在还没有工具知道它们为什么相关。配置和缓存隔离在示例目录，不改原有项目；结束后文件保留，退出 Bash 会话即可恢复原环境。

#### 2. 登记文件，不冒充正文复核

```bash
omd init 需求.md --json > init-spec.json
jq '.data | {ok, commit}' init-spec.json
```

`ok: true` 表示保存了文件基线。**init 不确认正文，不猜测实现，也不建立 link。**

此后的每次写入，都要先 `verify` 观察，再带上 `--expected` 凭据。这防止你在不知情时确认别人刚改过的内容。当前 CLI 要求手动传递 JSON；下面把这段重复操作集中起来，但不隐藏它。

<details>
<summary>展开并执行一次：observe 的实际定义</summary>

这是本教程的 Bash 函数，**不是 OMD 命令**：执行一次 verify、显示结果、取出 `data.expected`。它不提交、不确认、不自动重试。

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
  jq -e '.data.expected // error("没有可用观察，停止")' observation.json > expected.json
}
```

exit 1 可以是我们主动修改内容后的复核提醒，不等于没有可用观察。先阅读结果；出现 missing、采集失败、锁冲突等非预期情况就停下。不要自动刷新凭据重试，也不要连续粘贴后续所有命令。

</details>

```bash
observe
omd init retry.py --expected expected.json --json > init-code.json
jq '.data | {ok, commit}' init-code.json
```

两次 init 都应成功。此时只是登记了两个文件，还没有正文范围和关联。

#### 3. 说明这段代码为什么负责这条需求

选中需求正文的 `[0,9)`：从 0 开始，到第 9 个字符之前，正好是“最多重试 3 次。”，不含换行。text 模式按 Unicode 字符计数，不是行号或 UTF-8 字节数。

```bash
observe
omd commit commit 需求.md --range 0 9 --mode text \
  --reason '约定最多重试 3 次' --expected expected.json --json > spec-range.json
SPEC_ID=$(jq -er '.data.object.chain_root_commit_id' spec-range.json)
jq '.data.object | {chain_root_commit_id, position}' spec-range.json
```

`commit commit` 是当前真实语法：前者是子命令，后者是普通提交类型。`SPEC_ID` 是本次返回的范围身份，不要复制别人示例里的 ID。

接着选中 `MAX_RETRIES = 3` 的 15 个字符，并建立入向关联：

```bash
observe
omd commit commit retry.py --range 0 15 --mode text \
  --link-from "$SPEC_ID" --reason '用 MAX_RETRIES 表达需求中的重试上限' \
  --expected expected.json --json > code-range.json
CODE_ID=$(jq -er '.data.object.chain_root_commit_id' code-range.json)
CODE_TIP=$(jq -er '.data.object.tip_commit_id' code-range.json)
LINK_ID=$(jq -er '.data.link_records[0].link_id' code-range.json)
omd links --json > linked.json
omd links show "$LINK_ID" --json | jq '.link | {link_id, from: .full.source.object.root_commit_id, to: .full.target.object.root_commit_id}'
```

核对输出：一条 link 从 `SPEC_ID` 指向 `CODE_ID`，对应：

```text
需求：最多重试 3 次。 → 实现：MAX_RETRIES = 3
```

范围身份回答“哪个片段”，link ID 回答“哪条关系”，tip 是续改时使用的当前版本。关系的理由由你判断；OMD 不会因为变量叫 MAX_RETRIES 就证明程序正确。

#### 4. 改需求，先别动代码

```bash
printf '最多重试 5 次。\n' > 需求.md
observe
```

这次应看到 exit 1、`ok: false`，`dirty` 中有 `range:<SPEC_ID>`，原因包含 `in-range edit`。这是**已跟踪需求发生变化，需要复核**，不是程序崩溃。

沿关系找实现，而不是凭记忆搜文件名。link 的完整投影会解析每个端点——目标的 `resolved` 投影直接给出路径与范围：

```bash
omd links show "$LINK_ID" --json > linked.json
jq '.link.full.target.resolved | {project_relative_path, position}' linked.json
python3 -c 'from pathlib import Path; print(Path("retry.py").read_text(), end="")'
```

目标指向 `retry.py` 的 `[0,15)`，内容仍是 `MAX_RETRIES = 3`。**现在需要你判断：新上限是否接受？这段实现要不要改？**

#### 5. 接受新需求，再记录实现如何处理

本例决定接受 5 次。刚才观察中唯一预期问题是自己改动的需求；用这份凭据续改原范围：

```bash
omd commit commit 需求.md --id "$SPEC_ID" --range 0 9 --mode text \
  --reason '将约定的重试上限从 3 调整为 5' \
  --expected expected.json --json > spec-five.json
CHANGE_ID=$(jq -er '.data.commit' spec-five.json)
```

这里需求尚未续改过，链根恰好也是当前 tip；以后应查询最新 tip，不能一直用链根。记录新需求后，当前 `verify` 可能就返回成功，**不代表实现也已更新**。我们已看到代码仍是 3，必须继续处理。

```bash
printf 'MAX_RETRIES = 5\n' > retry.py
python3 -c 'from retry import MAX_RETRIES; print("当前上限：", MAX_RETRIES); assert MAX_RETRIES == 5'
observe
```

小检查打印 `当前上限： 5`。它只检验常量，不证明真实重试循环、异常处理或副作用正确；实际项目需要对应测试。

先保存实现的新版本，再单独记录适配。适配明确选择**哪条 link、哪次需求变化、什么处理理由**：

```bash
omd commit commit retry.py --id "$CODE_TIP" --range 0 15 --mode text \
  --reason '实现新的重试上限 5' \
  --expected expected.json --json > code-five.json
jq '.data | {ok, commit}' code-five.json
NEW_CODE_TIP=$(jq -er '.data.object.tip_commit_id' code-five.json)
```

```bash
observe
ADAPT=$(jq -nc --arg link "$LINK_ID" --arg change "$CHANGE_ID" \
  '{link_id:$link, changes:[$change], reason:"已核对新需求，将 MAX_RETRIES 从 3 改为 5，并检查常量值"}')
omd commit adapt retry.py --id "$NEW_CODE_TIP" --adapt "$ADAPT" \
  --expected expected.json --json > adapted.json
jq '.data | {ok, kind, commit}' adapted.json
```

看到 `kind: "Adapt"` 才是独立的适配记录。使用明确的 `commit adapt`；当前普通 `commit commit` 即使带 `--adapt` 返回成功，也不能据此声称记录了适配。

`--adapt` 不表示“全部清绿”：只处理这条关系上的这个变化，不替其他关联做决定。

#### 6. 核对结果，保留判断边界

```bash
observe
REVIEW_TIP=$(jq -er '.data.object.tip_commit_id' adapted.json)
omd log "$REVIEW_TIP" --json > history.json
jq '.data | {chain, selected: (.selected | {chain_root_commit_id, tip_commit_id, position})}' history.json
```

最终应看到 `ok: true`，dirty、locate、missing 和待处理项为空；历史保留原范围身份及新版本，完整查询结果在 `history.json`。`spec-five.json`、`code-five.json` 和 `adapted.json` 保存本次结果，理由和适配选择由 `.omd/` 保存。

你走完了 **建立关系 → 发现变化 → 找到实现 → 判断和检查 → 记录处理**。OMD 保存可追溯的关系与处理，不替你证明业务正确，也不意味着整个项目已覆盖。这个例子没有设置覆盖统计或规则，`import` 的用途见 [README](../../README.zh-CN.md#纳管文件与目录)。

改名、缓存和回归断言不混进首次教程。原自动验证脚本保留在 [examples/regression](../../examples/regression/README.md)，供维护者跑回归，不作为人类入门步骤。
