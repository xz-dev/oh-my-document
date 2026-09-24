# audit-ledger Specification

## Purpose

定义 review/audit 作为独立 append-only commit 链对象：从种子沿 link 图涂色、双方向独立判定、三态结论 patch 追加、wiki 式 commit 互链与 unclean→修复回链闭环，全部钉住特定 commit 而非最新状态。

## Requirements


### Requirement: An audit is an append-only linear commit chain with its own root

audit SHALL 作为独立线性对象实现：每个 audit 是一条以 init commit 为根的链，结论修订以 patch commit 追加（携带 `previous_id`），MUST NOT 原地覆盖既有记录。audit 链 SHALL 与 file/range 树、note 链共用同一套 commit 记录与链式实现，不引入第二套历史机制。audit commit SHALL 记录：种子（被审对象的 commit id 或对象 id）、涂色方向、正文（markdown）、结论状态与时间戳。引用任何 audit 的任何记录 SHALL 指向该链上的特定 commit id，MUST NOT 指向"该 audit 的最新状态"。

#### Scenario: A patch appends, never overwrites
- **GIVEN** audit A 已有结论 pass（commit a2）
- **WHEN** 用户以新证据修订为 fail
- **THEN** 追加 a3（结论 fail，previous 指向 a2），a2 保留可查
- **AND** 不存在被改写的 a2

#### Scenario: References pin a specific audit commit
- **GIVEN** audit A 当前链为 a1←a2，另一个 audit B 的正文引用 A
- **WHEN** A 追加 a3 之后查看 B 的引用
- **THEN** B 的引用仍然解析到它写下时指向的特定 commit（a2），不是 a3

### Requirement: Audit scope is a colored map with an explicit direction

`omd audit add <seed>` SHALL 记录涂色方向 `--direction both|upstream|downstream`（默认 `both`）。`omd audit show` SHALL 沿 seed 按该方向遍历 link 图，得到涂色区域（被审子图）；正向与反向 SHALL 作为独立检查分别报告，MUST NOT 混合为一个结论。对涂色区域内每条边（link）SHALL 执行 L0–L2 健康判定，每个点（端点）SHALL 核活性；检查结果 SHALL 作为证据附加到 audit 记录，不改变被审对象自身的状态。

#### Scenario: Direction is explicit and defaults to both
- **WHEN** 用户运行 `omd audit add <seed>` 不带方向
- **THEN** audit 记录 direction 为 both，show 时正向反向分别检查分别报告

#### Scenario: Upstream and downstream are checked separately
- **GIVEN** seed 的一条 link 上游有未处理义务、下游健康
- **WHEN** 用户运行 `omd audit show <audit-id>`
- **THEN** 报告中 upstream 部分列出该义务，downstream 部分独立报告健康
- **AND** 不把两个方向的结果合并成一个总体结论

### Requirement: Audit conclusions are three states appended as patches

audit 结论 SHALL 是三态之一：`pass`、`fail`、`pending`（暂定）。`omd audit pass|fail|pending <audit-id>` SHALL 各自追加一个结论 patch commit。pending SHALL 是合法的一等结论——"还没定"不是缺失。audit 相关命令的退出码：仅当检查发现 broken（结构损坏）SHALL exit 1；存在 fail 结论或 pending SHALL NOT 影响退出码。

#### Scenario: Pending is a first-class conclusion
- **WHEN** 用户运行 `omd audit pending <audit-id> --text "等上游合并"`
- **THEN** 追加 pending 结论 commit，命令成功
- **AND** 该 audit 在 list --status pending 中可查

#### Scenario: A fail conclusion is a record, not a crash
- **GIVEN** audit A 有 fail 结论
- **WHEN** 用户运行 `omd audit list`
- **THEN** 命令 exit 0 并如实列出 A 的 fail 状态

#### Scenario: Broken findings fail the audit command
- **GIVEN** 涂色区域内存在端点 key 损坏的 link
- **WHEN** 用户运行 `omd audit show <audit-id>`
- **THEN** 命令 exit 1 并报告 broken 的 link 与原因

### Requirement: Dual references — link objects and payload fields — are both checked

修复 commit 与 audit 之间、audit 与 audit 之间的引用 SHALL 同时支持两种机制并存：(a) 真 Link 对象，端点类型扩展到 audit/note commit（线引用）；(b) 载荷字段引用（钉特定 id，note 模式）。`omd audit show` SHALL 对两者分别检查：link 端点活性（线接错：端点撤走或接错对象）与载荷引用解析（点画错：引用指向不存在的 commit 或坐标与 link 端点不一致），并 SHALL 执行交叉一致性检查——同一处引用在两种机制下 MUST 指向同一状态，不一致时如实报告。

#### Scenario: A wrong wire is caught by the link check
- **GIVEN** 修复 commit 通过 Link 对象连接 audit A，但端点被 reset 撤走
- **WHEN** 用户运行 `omd audit show`
- **THEN** link 检查报告端点 withdrawn，报告指出线的问题
- **AND** 不因此宣称载荷引用也有问题（两者独立报告）

#### Scenario: A wrong dot is caught by the payload check
- **GIVEN** audit 正文引用 `audit:<id>` 的特定 commit，但该 id 不存在或坐标与既有 Link 端点不一致
- **WHEN** 用户运行 `omd audit show`
- **THEN** 载荷引用检查报告解析失败或不一致，报告指出点的问题

#### Scenario: Cross-consistency flags mixed stories
- **GIVEN** 修复 commit 的 Link 对象端点指向 audit commit a2，但载荷 reason 引用 a3
- **WHEN** 用户运行 `omd audit show`
- **THEN** 交叉一致性检查报告两种机制指向不同状态，如实呈现差异

### Requirement: Audit wiki references pin states in markdown

audit 正文（markdown）SHALL 允许引用其他 audit 链上的特定 commit id（如 `audit:<commit-id>`）；解析时 SHALL 验证该 id 存在于对应 audit 链上，MUST NOT 解析为"目标 audit 的最新状态"。audit 互链构成 audit wiki：每条边都是特定历史状态的钉住引用。

#### Scenario: A wiki reference survives target growth
- **GIVEN** audit B 正文引用 audit A 的 a2，随后 A 追加 a3
- **WHEN** 用户查看或解析 B 的引用
- **THEN** 引用仍解析到 a2 的内容与状态
- **AND** 不自动跟随到 a3

### Requirement: The problem workflow closes through unclean and dual references

audit 发现问题 SHALL 能通过既有 `commit unclean` 机制凭空造脏并以 audit id 作为理由索引；修复 commit 产生后 SHALL 能以 Link 对象与载荷字段双引用链回 audit 特定 commit id。整个闭环——发现问题、造脏索引、修复、回链——SHALL 只使用既有命令语义，不引入新的写入路径。

#### Scenario: A finding becomes an indexed dirty obligation
- **GIVEN** audit show 发现某范围有问题
- **WHEN** 用户运行 `omd commit unclean <path> --reason "audit:<audit-commit-id>"`
- **THEN** 产生既有的 unclean 义务（verify 报 dirty），理由可解析回该 audit commit
- **AND** 不需要新命令

#### Scenario: The fix links back to the audit state
- **GIVEN** 修复 commit 已发布
- **WHEN** 用户为它建立回链（Link 对象端点 + 载荷引用）
- **THEN** audit show 能从该修复 commit 沿两种引用回到 audit 特定 commit
- **AND** 两侧引用指向同一 audit commit id

### Requirement: Audit listing filters by status and time windows

`omd audit list` SHALL 支持：`--status pass|fail|pending`；audit 自身时间窗 `--start`/`--end`（按 audit 链上 commit 的时间戳过滤）；涂色区域内 link 端点版本修改时间窗 `--touched-start`/`--touched-end`。时间数据 SHALL 只来自 commit 记录的既有时间戳字段，是查询投影，MUST NOT 引入新的时间权威。分页契约与 links list 一致（默认 20、上限 100、提示性游标）。

#### Scenario: Status and own-time filtering
- **GIVEN** 三个 audit：一个 pass（9月1日）、一个 fail（9月10日）、一个 pending（9月20日）
- **WHEN** 用户运行 `omd audit list --status fail --start 2026-09-05`
- **THEN** 只列出 9月10日的 fail audit
- **AND** 时间窗对时间戳的读取不改变任何 commit 记录

#### Scenario: Touched-window filtering reaches endpoint versions
- **GIVEN** audit A 的涂色区域内某 link 端点的钉住版本 commit 时间戳在 9月15日
- **WHEN** 用户运行 `omd audit list --touched-start 2026-09-14 --touched-end 2026-09-16`
- **THEN** audit A 出现在结果中
- **AND** 过滤是只读投影，不触发内容采集
