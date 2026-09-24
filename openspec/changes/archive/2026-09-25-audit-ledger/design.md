# Design

## Context

见 proposal.md。已有地基：Commit 记录 + previous_id 链 + reset 语义（src/records/）；link 对象与端点（state.links，`range:`/`file:`/`peer:` key）；L0–L2 健康判定与 SCC 分层（src/relations/linkhealth.rs，link-query change）；note 平面文件（src/records/notes.rs）；`commit unclean` 凭空造脏。

## Goals / Non-Goals

**Goals**
- audit/note 成为真线性 commit 链，与 file/range 共用一套链实现。
- 涂色地图检查：显式方向（默认 both），正反分开报告。
- 双引用（Link 对象 + 载荷字段）都检查 + 交叉一致性。
- 时间过滤是既有时间戳的只读投影。
- 旧 note 平面格式明确拒绝，当不存在。

**Non-Goals**
- 不自动迁移旧 notes 数据（所有者手动）。
- 不改变 unclean/clean--stop/三态 marker 语义。
- 不做 audit 的自动重跑/定时/触发器——audit 是人/Agent 主动动作。
- 不把 pass/fail/pending 写进被审对象状态——结论只活在 audit 链上。

## Decisions

### D-1 audit/note 链复用 Commit 记录，新增对象 key 前缀

`audit:<chain-root>` / `note:<chain-root>` 作为 node key，与 `range:`/`file:` 平级。每条 audit/note 是**一条**链（不像 file 挂多 range 子链）：init commit 为根，patch 追加。复用 `Commit` 结构（kind 扩展 `AuditInit`/`AuditPatch`/`NoteInit`/`NotePatch`）、`previous_id`、`tips` map、reset 语义。不新建第二套历史文件格式。

替代：独立 `audits/` 目录 + 自定义记录。拒绝：违背"同一套树实现"，reset/log/gc 全要再造一遍。

### D-2 涂色 = 显式方向的 link 图遍历，逐边判定复用 linkhealth

`audit show` 从 seed 出发 BFS/DFS：`both` = 沿 source→target 和 target→source 两个方向分别遍历，产出两个独立子图与两份判定报告；`upstream`/`downstream` 只走一个方向。遍历以节点为界（不进入端点 commit 内容）。每条边跑既有 `judge_link`（L0–L2），每点跑 `endpoint_alive`。结果快照作为证据附在 show 输出，不写回 audit 链（audit patch 只记结论与正文）。

理由：正反向是不同的事（用户决定）——上游义务与下游责任分开涂色分开查，混合会让"哪边出问题"不可判。

### D-3 双引用 = Link 端点扩展 + 载荷钉 id，交叉一致是检查项不是写入门

写入侧：修复 commit 可建 Link 对象（端点 `audit:<id>`）也可只写载荷 `reason: "audit:<id>"`，两者都合法、不强制成对。检查侧（audit show / verify）：对每处"同时有两种机制"的关系做一致性比对，不一致报检查失败。

不在写入时强制配对的理由：note 模式（只载荷）先于 link 扩展存在，强制成对会把所有既有载荷引用变成非法；检查面捕获不一致已经满足"两个都要检查"。

### D-4 audit wiki 引用 = 正文内的 `audit:<hex>` 记号 + 解析验证

正文是自由 markdown，不引入新解析器；引用记号是约定字符串 `audit:<commit-id>`。解析（audit show 的引用检查部分）扫描正文中的记号，验证 id 存在于某条 audit 链上，输出解析结果（指向哪条链哪个 commit、该 commit 的结论）。不存在/坐标错 → 点画错报告。不渲染、不构建反向索引文件——解析即时进行，结果是查询投影。

### D-5 三态结论 = AuditPatch commit 的载荷字段

`pass`/`fail`/`pending` 三个 CLI 动词都 append 同一种 AuditPatch commit，载荷 `{conclusion, text, timestamp}`。pending 一等公民（"还没定"合法）。退出码：仅检查发现 broken exit 1；结论本身不影响退出码（记录不是崩溃）。

### D-6 note 迁移 = 拒绝旧格式 + 手动重建

读路径遇到 `notes/<id>.toml` 无法按链式解释时 → 报错指路径。不写回。本仓库 `.omd` 自管理数据的手动迁移由所有者另行执行（浮浮酱将在实现完成、主人授权后处理本仓库自身数据）。

### D-7 时间过滤 = commit 时间戳的只读投影

`--start/--end` 过 audit 链 commit 的 `timestamp` 字段；`--touched-start/--touched-end` 过涂色区域内 link 端点**钉住版本 commit** 的时间戳。全部现读现过滤，无索引、无缓存、不写任何东西。与游标一样是查询投影不是权威。

## Risks / Trade-offs

- **CommitKind 扩面**：state schema 增加 audit/note tips。`#[serde(default)]` 既有惯例，旧 store 读到新字段为空——兼容（空 = 无 audit/note）。
- **旧 store 拒绝策略的边界**：只有"读 note 集合"的命令遇到旧文件才报错；不读 note 的命令（verify 等）不受影响——需在实现时明确哪些命令算"读 note 集合"。
- **audit show 的遍历成本**：涂色区域可能大（both + 大连通分量）。已有 L0–L2 便宜性保证；上限靠分页报告不靠截断遍历。病态大图时遍历本身 O(V+E)，可接受。
- **双引用一致性检查的假阳性**：载荷引用允许指向 link 端点之外的 commit（正文引用历史状态合法）——一致性检查只比对"同一处关系"的两种机制，不做全文记号与 link 的全量交叉。边界要在实现时写清测试。
- **本仓库自管理数据迁移**：主人手动，浮浮酱不自动动 `.omd`。

## Open Questions

（无——存储模型、方向语义、双引用、note 迁移、退出码均已与用户确认。）
