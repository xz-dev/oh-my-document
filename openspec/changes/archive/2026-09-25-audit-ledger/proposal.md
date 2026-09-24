## Why

分层检查错链（`omd links` 的健康判定）只回答"现在哪些边有问题"，但审计是跨越时间的工作：发现问题 → 造脏索引 → 修复 → 回看结论，这一整段过程没有承载对象。需要一等公民的 audit 对象——与 note 同构的独立线性 commit 链——记录审计的范围（涂色地图）、结论（pass/fail/pending 三态）和理由（markdown 正文，可互链成 audit wiki），并通过既有 unclean 机制闭合"发现问题→修复→回链"的工作流。

## What Changes

- 新增 audit 对象：独立线性 commit 链（与 file/range、note 树共用同一套 commit 链实现），append-only，每次结论修订追加 patch commit，不原地覆盖。种子（seed）是任意 commit id / 对象 id；涂色方向显式选择 `--direction both|upstream|downstream`，默认 `both`，正向与反向分开涂色分开检查。
- audit 三态 `pass` / `fail` / `pending`（暂定），结论作为 patch commit 追加；audit 正文是 markdown，正文内可引用其他 audit 链上的**特定 commit id**（audit wiki，永不指向"最新"）。
- `omd audit show` 沿涂色方向走 link 图得到被审子图（涂色区域），对每条边做 L0–L2 健康判定、每个点核活性，并把检查结果作为证据附到 audit。
- **双引用模型**：修复 commit 与 audit 之间同时使用真 Link 对象（端点扩展到 audit/note commit）和载荷字段引用（note 模式的钉 id）。两者都要检查——线接错（link 端点接错/撤走）和点画错（载荷引用指向不存在或与 link 端点不一致）分别暴露，`audit show` 做交叉一致性检查。
- **note 升级为真 commit 链**：note 从 `notes/<id>.toml` 平面文件+seq 迁移到与 audit 同构的线性 commit 链。旧格式 `notes/*.toml` 明确拒绝（当不存在，不读不写不迁移）；本仓库自管理数据由所有者手动迁移。
- `omd audit list` 过滤：`--status` 三态、audit 自身开始/结论时间窗（`--start`/`--end`）、涂色区域内 link 端点版本修改时间窗（`--touched-start`/`--touched-end`）。时间来源是 commit 记录的 RFC3339 时间戳，是既有字段的查询投影，不是新权威。
- 工作流闭环：audit 发现问题 → `commit unclean <path> --reason "audit:<id>"` 凭空造脏并索引 → 修复 commit 产生 → 修复 commit 通过双引用链回 audit 特定 id。
- audit 相关命令退出码：仅 broken（结构损坏）exit 1；pass/fail/pending 是记录不是崩溃，exit 0。
- `omd audit` CLI 面：`add` / `list` / `show` / `pass` / `fail` / `pending`，与 `omd note <action>` 同一模式。

## Capabilities

### New Capabilities
- `audit-ledger`: audit 对象生命周期——线性链、三态结论、涂色地图检查、双引用模型、时间过滤、unclean 工作流闭环、退出码语义。
- `note-ledger`: note 升级为真 commit 链——新链实现、旧平面格式明确拒绝、手动迁移路径。

### Modified Capabilities
- `change-review`: Link 端点从 range 扩展到 audit/note commit（线引用），跨引用一致性检查进入检查面。

## Impact

- `src/records/`: audit 与 note 的 commit 链记录（复用 Commit 结构与 previous_id 语义）；state 增加 audit/note 树根与 tip。
- `src/relations/`: link 端点类型扩展（`audit:`/`note:` 前缀对象 key）；涂色子图遍历（按 direction）；双引用交叉一致性检查。
- `src/main.rs`: `Cmd::Audit` 动作词；note 命令面改造。
- 迁移：旧 `notes/*.toml` 拒绝读取；本仓库 `.omd` 自管理数据手动迁移（另行执行，不在本 change 的自动化范围内）。
- 测试：audit 链、涂色、双引用、note 拒旧格式集成测试。
