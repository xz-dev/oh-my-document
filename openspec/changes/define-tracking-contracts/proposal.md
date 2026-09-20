## Why

OMD 的需求与逐轮决定需要整理成可验证的 Rust 核心与 CLI 契约。本变更继续使用 `define-tracking-contracts`，交付 proposal、design、四份行为规格及实施任务；D-33 已记录末尾插入的最终选择和 E-1～E-8 的明确接受。本轮仍无可执行产品或产品测试结果，规划完成不等于实现完成。

## What Changes

- 建立虚拟根、项目、文件式来源对象与 range 的提交记录；file、command、git 是内容来源方式，不因此新增挂载层级。import 是项目下独立的覆盖统计/tag 分支，不拥有文件/range。import/remove 不修改磁盘文件，也不删除独立跟踪记录。
- 显式 init 文件；范围以 `commit --range` 新建，以 `--id` 在原跟踪上继续提交。允许重叠与同坐标的独立范围。
- 已记录的 file/command 内容保留完整副本；显式 git 引用复用本地已有 Git 提交中的完整历史内容，记录 Git/OMD commit 关联，不另存完整副本。Git 对象缺失时报错，不能用当前文件冒充旧内容；这只限定 git 方式的历史恢复保证，不改变 file/command 的自持久化要求（D-29）。
- 对真实文件，verify 默认读取当前登记路径的完整现状，包括未提交修改，不由 HEAD、分支或 index 选择当前内容。旧内容可由确切 Git 版本提供，但不能取代当前路径；统一用内容 hash 与 Rust 内 Myers 比较完整旧新内容。位置迁移和紧贴范围末尾的插入都须复核；即使原句文字与位置未变，也不能认定新增内容与其无关而保留确认。不自动扩大范围，不将整文件 hash 改变解释为全部 range 变脏（D-31、D-33）。坐标采用 0 起点左闭右开；全删可显式提交空区间，保留身份但不贡献覆盖，也不拿空目标填补非空内容的 link 覆盖。
- 三种来源可对已记录版本显式 replace，不通过新增 OMD commit 叠加设置。目标必须提供同一份完整内容；不一致则拒绝且原状不变，不能只换后续读取入口。成功将 O1 的旧 X 改由 Git 提供，不把当前文件观察改成读取 Git 树。既有 commit ID 与原始输入、range、link ID、note 及其他历史恢复依据保持不变；不自动清除副本。经 commit 选择其完整来源版本的恢复 binding，列出共享同一版本的受影响记录，不批量改绑仅内容 hash 相同的其他版本；显式 command replace 授权本次采集一次。副本仅由显式 `gc --content` 在保护和恢复检查通过后释放（D-30、D-33、E-5）。
- 每次范围修改产生新 commit。range 自身 reset 到合法普通目标时保留目标点并恢复该点的范围；BEGIN/END 是占位标记符 commit，不是独立正文恢复版本，reset 撤下标记及后续，统一落到其直接前驱并警告（D-27）。块内普通成员禁止直接作为目标。文件 reset 精确恢复目标记录的范围版本；若任一子范围需恢复到块内普通提交，整次拒绝，不部分恢复或改选边界。被撤下的记录成为 dangling，磁盘内容、历史和 note 不被物理删除。见 D-22。
- 区分适配提交、源端按分支 clean 阻断及 unclean 叠加责任；部分处理不消除其他责任。只有空、脏、已确认三态；skip 不赋予确认资格。
- link 连接 range 跟踪对象，不将各次 commit 版本误当作独立端点。每个关联实例有自己的 link ID；允许方向与端点相同的多个 link 共存。创建 link 必须显式指定，适配必须点名被处理的 link ID 并给出对应理由，不能从文本或端点猜测。已有关系随范围版本推进保持身份，无须每次重建；仅处理 L1 不会处理同端点的 L2。允许环并保证遍历终止，必要版本依据直接或间接断裂的诊断与回退重建规则保留。见 design.md D-24。
- `omd commit atomic begin/end` 沿单个 range 的提交链归组，允许嵌套；end 关闭最近未闭合的 begin。各次成功提交立即生效，verify 检查所有有效块闭合。任何层级的 begin/end 都可作为 reset 目标，统一沿 previous_id 退一步，不受外层块另行限制，也不递归跳过前驱标记。普通内部成员仍禁止直接 reset。`--link-from` / `--link-to` 可将范围修改与新 link 操作组成同链块；两方向可混用、可重复，单命令内同方向同端点重复项报错，不做全局去重。中途失败保留已成功项、不自动补 end；用户显式继续或按边界 reset。首个无前驱 BEGIN 的 reset 产生空链、撤下范围挂载并返回 actual_id=null，不是 reset 虚拟根。单次记录的持久发布与整块逐次生效分开，按 E-3/E-6 实现。
- 记录文件 rename、copy 初始化与 target 为 null 的 tombstone。rename 只改记录路径，保留范围与 link，不在 commit 中跑 diff。missing、dangling、unreachable_link、conflict 是诊断，不是新标记状态。
- 每个 commit 记录时间，并按盐、同节点前驱、时间戳、完整内容及操作输入生成 SHA-256 ID；project_id、note、缓存与派生状态不加入 ID。支持显式提交时间，用于用户手工重放；不提供合并工具。
- note 独立持久化，每条有 ID 和时间；修改/删除以新的 patch/delete 记录表达，不原地抹除历史。unclean 平面列出全部；显式 gc 保护有效记录引用的 dangling，清理 commit 时一并清理其 note。
- command 与文件只在内容取得方式上不同：显式 init 时执行固定 executable 与 JSON 字符串 argv 一次，在所属项目根取得正常 exit 0 的完整 stdout；成功并通过写入检查才产生初始化 commit，失败不产生。后续共用跟踪逻辑；verify/check 均使用本次 `--run-command` / `--run-command=false` 优先于用户 `VERIFY_COMMAND_AUTO_RUN` 的同一规则，默认不执行，clean 不重跑命令（D-28、D-32）。
- 项目 alias 只映射本地目录，不自动获取远端；允许显式登记后跨独立权威元数据目录引用，保留各自记录，不要求合并或隐式复制，核对参与方版本与可用性。Git 引用定位历史内容，不把 alias 变成固定快照，也不改变真实文件的当前路径观察。可选 Git 身份核对失败时要求用户修正登记，不提供 force 旁路。
- `check` 按文件/文件夹中的应计非空内容统计覆盖率；文本不计空格、制表符、换行等空白，但不改写原文、坐标或完整内容比较。tag 关系的 link 覆盖要求全部应计内容至少被合格关联范围覆盖一次。多个范围可共同覆盖，重叠位置只计一次，不以 range 数、文件数或一条边的存在代替内容覆盖。只有未过期已确认范围贡献有效覆盖；空文件及全空白文本显示 100%，不额外规定两套独立报告模式（D-32）。
- tag 命名检查集的 warn/fail（默认 fail）作用于用户请求的 check。覆盖不足不成为 verify 的强制失败条件，覆盖完整也不清除未处理责任或替代必要引用、来源与 ATOMIC 闭合检查。check 不自动确认、修复或授予 command 额外执行许可。目录 tag 递归继承并叠加；单向规则不禁止额外反向 link，双向则分别要求两个方向覆盖完整。持久化采用不可变 TOML 记录、完整来源字节和 state 发布索引；OS 锁与 expected 凭据拒绝并发或旧依据写入。跨存储先在被引用方保存保护凭据，再发布所属方业务记录；相关目录失联时不凭缓存通过检查或释放保护，副本成为可写权威目录须显式登记（E-2～E-5）。
- 提供 `omd list --dangling`、`omd log <commit-id>`、可选起点及 project/file 层级限制的 `omd tree`；所有命令支持 `--json`。观察和查询不能隐式执行来源程序、恢复历史或清理数据。

## Capabilities

### New Capabilities

- `managed-content-tracking`: 文件式来源与 range 注册、当前路径观察与完整旧内容取回、Git 历史复用与恢复边界、同内容 replace、Myers 范围复核、路径生命周期、import 覆盖统计、check 与 verify 的边界及单写者持久化边界。
- `change-review`: 带 link ID 的定向范围关联实例、显式对应 ID 与理由的适配、阻断、unclean、ATOMIC 块及组合简写、reset 边界、断链诊断、提交身份、note、gc 和 log/tree/JSON 查询。
- `command-verification`: command 初始化采集与项目根工作目录、固定 argv、verify/check 共用执行许可、完整输出及失败保留，共用文件式版本复核。
- `local-project-links`: 本地项目登记和 alias、可选 remote 身份核对、跨项目范围关联，以及由 check 执行的 tag 命名关系覆盖检查。

### Modified Capabilities

无。`openspec/specs/` 尚无既有 capability specs；原需求与场景位于 `docs/`，本变更使用 ADDED requirements。

## Impact

- 本轮只生成或修订本 change 的 proposal、design、delta specs 和 tasks，不初始化 Rust/Lean 工程、不安装 hook、不写产品代码，也不执行 Git 提交或推送。
- 后续实施涉及 Rust 核心与 `omd` CLI、权威文本记录、来源内容/必要定位和可重建 SQLite 索引。内容判定仍使用 hash 与 Rust 内 Myers，不依赖 Git diff、编辑器、AI agent、OpenSpec 或 Mermaid；用户显式采用 Git 引用保存的旧版本需要本地 Git 读取能力及对应仓库对象，但真实文件当前内容仍由路径读取；其他来源不因这一扩展而新增 Git 依赖。
- 保留 R-01～R-23 中与本变更相关的一般边界；D-29 明确限定了 git 方式的无 Git 内容恢复保证，理由是复用已有 Git 内容而不再复制。可选 Lean 产品 skill（R-24～R-28）、worktree 指南（D-13）、发布与许可证另行交付，不因本轮规划被取消或被宣称已实现。
- `docs/handoff.md` 与 `docs/open-questions.md` 已有状态指针；其他基线正文仍需按新决定核对，不能把所有旧问题都视为已解决。
- D-33 已明确接受工程基线，OQ-P1～P9 在本 change 内关闭；四份规格及带逐项验证条件的 tasks 是后续实施依据。具体库版本、许可证、发布及可选产品 skill 不在本轮实施授权内。文档结构校验不等于产品测试通过。
