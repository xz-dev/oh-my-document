---
name: omd
description: 使用 OMD 管理规范、文档、代码和测试的范围关联；适用于初始化与 import、建立 link、修改后的复核与适配、覆盖规则检查，以及诊断过期依据或缺失来源。根据真实使用经验维护，不把关联检查当成语义证明。
---

# OMD 工作流

## 先确定本次操作

1. 确认项目根、权威 metadata 和二进制。运行 `omd --help` 与目标子命令的 `--help`；同为 `0.0.0` 不证明两个二进制相同。
2. 查看现有 metadata、本机映射和工作区变化。已有或损坏 store 不重新 init 覆盖；身份冲突先停下来确认。
3. 与用户明确 import 范围、来源程序执行许可、覆盖规则及写入授权。默认 `.omd/` 是权威，默认用户配置保存本机位置，缓存不是唯一基线。
4. 在真实项目做有用的工作；恢复、GC、副本激活等危险演练放隔离环境。Git 提交、push、安装 hook、归档 OpenSpec 分别取得授权。

完成条件：根、metadata、可执行文件、纳管范围都明确。完整能力与实践状态见 [功能分支](references/capabilities.md)。

## 初始化与纳管

```bash
OMD_BIN=/absolute/path/to/omd
ROOT=/absolute/path/to/project
META="$ROOT/.omd"
"$OMD_BIN" --root "$ROOT" --meta "$META" init docs/spec.md --json
```

路径替换为真实已存在文件。首次 init 建立文件记录和完整内容基线，不确认正文。已有 store 中的后续 init 也是需要凭据的写入。

`import` 是持续统计范围，不是批量 init，也不自动生成范围或 link。显式 import 文件只纳管自身；import 目录持续递归发现成员，新增文件进入分母但仍是未标记。`remove` 只撤统计，不删除来源或历史。不要假定 `.gitignore` 自动生效；需要时显式使用 `--exclude`/`--include`。可以主动 import `.omd/`，但自身写入不自动确认，不能假定自跟踪会收敛。

使用支持单文件 import 的当前构建。统计对象与正文对象独立：先 import 后 init、先 init 后 import 都可行；remove 只撤统计。核对 `data.check.files` 的 scope/file 和 tracked，不仅看退出码。同一路径存在两种对象时，tag 等通用操作用 `--id` 明确当前 tip。旧二进制无法读取同路径双对象，会明确拒绝；不要混用版本或手工删记录解锁。

## 每次写入都先观察

```bash
"$OMD_BIN" --root "$ROOT" --meta "$META" verify --json > "$OBSERVATION"
python3 - "$OBSERVATION" "$EXPECTED" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    result = json.load(f)
expected = result['data']['expected']
assert expected
with open(sys.argv[2], 'w') as f:
    json.dump(expected, f)
PY
"$OMD_BIN" --root "$ROOT" --meta "$META" import docs \
  --expected "$EXPECTED" --json
```

`OBSERVATION`/`EXPECTED` 放本机操作证据目录，不写进共享 skill，不复用上次操作的凭据。保存命令、退出码和完整 JSON。上例适用于 verify 成功；若 exit 1，先检查 dirty、missing、unverified、open_blocks 和 diagnostics，再决定是否有足够依据进行明确修复。不要因失败直接丢弃 JSON，也不要无视失败继续批量写入。

- exit 0：成功；warning 不等于失败。
- exit 1：检查失败或不完整；查看具体对象与理由。
- exit 2：用法/格式错误；检查输入。
- exit 3：版本冲突；停止，重新核对用户意图和变化，不能自动刷新凭据重试。
- exit 4：锁冲突；保留单写者，不删除锁绕过。
- exit 5：I/O/执行失败；核对真实部分发布，不假定零变更。

完成条件：每次写入携带独立新观察，返回结果与实际状态一致。

## 选择真实范围并建 link

1. 阅读具体规范、实现和直接测试；只标记已复核的程序单元，不整文件刷覆盖率。
2. text 使用实际编码解码后的 Unicode scalar 序号，0 起点、左闭右开；不是行号、UTF-8 字节或 UTF-16 单元。按原字节读取再解码，保留 BOM/CRLF；Python 默认文本读取可能转换换行。
3. byte 按原始字节计数，不解码。确认 `0 <= start <= end <= 长度`，核对切片内容。
4. 对每个文件显式 init；每个范围写入前重新观察。

```bash
"$OMD_BIN" --root "$ROOT" --meta "$META" commit commit docs/spec.md \
  --range "$START" "$END" --mode text --reason '说明本段实际复核的契约' \
  --expected "$EXPECTED" --json
```

从结果的 `data.object.chain_root_commit_id` 取得对象身份。随后对实现范围使用 `--link-from "$SPEC_ROOT"`；测试范围可从实现范围建立入向 link。理由必须说明关系和证明边界，不能把测试存在称为全部规范已实现。

新建范围不传 `--id`；续改传当前 `tip_commit_id`，不是始终使用链根。link 有独立 ID，后续适配按 link ID 和选定变化处理。同坐标不等于同一对象。组合可能产生 BEGIN/正文/link/END；有效正文与当前 tip 分开。

完成条件：`omd links --json` 概要确认 link 存在（total 增加）；`omd links show <link-id>` 查单链完整投影（含端点解析路径与范围）；`log <id>`/`tree` 可追溯。身份、位置、版本取结构化字段，不解析展示标签。

## 修改后的复核

**先过滤，再逐条审**。有 `--difftastic` 可用时，第一步永远是过滤：

```bash
"$OMD_BIN" --root "$ROOT" --meta "$META" verify --difftastic --json > "$OBSERVATION"
```

`data.dirty` 只留结构树有变化（或无法分类）的范围——这是高优先级队列，逐条人审/深审。`data.cosmetic` 列出被判定结构无变化的范围，是低优先级的收尾批扫对象。**收尾不等于丢弃**：逐条审完 dirty 后，用 `commit cosmetic <path>` 在写锁下对凭据钉住的版本重分类并批量续改，证据随提交落库。

三条纪律写死：

1. **永远先过滤**。第一反应是 `--difftastic`，不是裸 verify 再人肉分桶——格式化风暴不该占用注意力。
2. **收尾不跳过**。cosmetic 桶最后仍要 `commit cosmetic` 过一遍；完成定义仍是不带 `--difftastic` 的 verify exit 0。
3. **工具不可用诚实降级**。difft 缺失/失败 → 相关范围归 `unclassified` 留在 dirty，退回全人工复核。明说"过滤不可用"，不装作过滤过了。

无 `--difftastic` 时退回旧路径：`verify --json` 定位受影响范围，再阅读新旧内容和关联方。需要坐标修订时显式续改已有范围；需要适配时明确选择 `link_id`、变化 commit 列表与理由，参见功能分支。按真实修改逐步积累该流程的使用证据，不为了演示修改生产代码。

`verify` 的 `locate` 也必须查看：本次真实规格编辑曾使已关联场景移动，检查返回失败但 dirty 为空；这仍是待复核，不能漏看。正文仅 init、尚无范围时，不报告 dirty 也不等于完成语义复核。

`check --json` 的覆盖缺口不等于 verify 的来源/关联失败。text 与 byte 分别统计；未知分母不能算零或 100%。脏范围、未处理关联和开放块不能靠覆盖率或 skip 清除。init/import/link 不证明语义等价；需要的测试仍单独执行。

**difftastic 的边界**：它只对有语法树的代码文件（`.rs`/`.py`/`.json` 等）有效；`.md` 等文档走文本回退，纯空白也报 `changed` 留在 dirty——本仓库的文档范围不会因过滤而减载，这是预期行为不是 bug。

完成条件：报告实际通过项、未覆盖范围、未处理责任及未执行验证；不为绿色结果降低规则或批量 clean。

## 审计闭环

发现问题后建立可追溯闭环，不靠口头记录：`omd audit add <seed> [--direction both|upstream|downstream] [--text]` 开 audit 链（默认 pending）；`audit show <id>` 沿种子涂色 link 图（both 分正反两图），逐边判 L0–L2；`audit pass|fail|pending` 追加结论 patch；`audit list --status/--start/--end/--touched-start/--touched-end` 过滤。正文可写 `audit:<commit-id>` wiki 引用钉住具体 commit；Link 端点可指向 audit/note 链，双引用不一致报 mismatch。仅结构性 broken exit 1。`commit unclean --reason "audit:<id>"` 造脏索引，修复 commit 回链闭环。详见 [capabilities](../../skills/omd/references/capabilities.md)。

## 在本仓库使用

本仓库实践与范围入口见 [自管理记录](../../docs/omd-self-management.md)。项目特定 ID 存放在那里及 `.omd/`，不复制到通用操作模板。此 skill 是仓库内首版，尚未全局安装；让其他 Agent 使用时显式提供本文件或按宿主支持的 skill 安装方式加载。
