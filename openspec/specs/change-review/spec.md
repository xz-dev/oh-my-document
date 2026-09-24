# change-review Specification

## Purpose

定义用户如何用范围提交接续或阻断变更责任、回退范围记录、检查失效依据以及查看历史和评论。确保局部处理不会抹掉其他责任，历史存在不会被误认为当前有效，也不以查询或注释代替复核。

> 规划基线：link 身份、嵌套 ATOMIC、逐次生效及占位标记退一步均已确认；D-33 进一步接受 E-1～E-8 的工程契约。以下要求和场景用于后续实施验收，不表示已经运行产品测试。

## Requirements

### Requirement: Links connect range tracking objects

`commit link` SHALL 连接 source range 到 target range，而非把两次 commit 版本当作独立的关系端点；不接受文件到文件的直接 link。range 的正常后续提交 SHALL 保持既有 link 的身份，不要求重新 link，也不隐式创建另一个 link。挂载关系、同范围提交前驱、范围之间的关联及复核版本依据 SHALL 被区分；关系仍存在不能使 dangling 的必要依据重新有效。反向查询 SHALL 不抹掉关系的语义方向。

#### Scenario: File IDs cannot substitute for selected ranges
- **WHEN** 用户试图直接 link 两个文件对象而未指定范围
- **THEN** 系统拒绝该关联并要求范围对象
- **AND** 不自动把整个文件包装为一个已确认范围

#### Scenario: One link persists across successive range versions
- **GIVEN** L1 连接范围 A 到 B，A 的 a2 变化已由 B 的 b2 通过显式指定 L1 和理由完成适配
- **WHEN** A 又提交 a3
- **THEN** B 有沿同一个 L1 处理 a3 这次变化的责任
- **AND** 不要求重建 L1，不因 a1/a2/a3 是不同 commit 而增加关联实例，也不将处理 a2 当作处理 a3

### Requirement: Link instances have distinct persistent identities

每个显式创建的 link 实例 SHALL 具有可区分的持久 link ID。系统 SHALL 允许方向、source range 和 target range 都相同的多个关联实例共存，MUST NOT 全局按端点合并或拒绝它们。关联的 link ID SHALL 与记录其创建或适配的 commit 身份分开表达，不因范围版本推进而变成另一个关联实例。实例的创建记录退出有效历史时，它 SHALL 不再计为当前有效关联，但创建记录及其中的 link ID SHALL 保留供历史检查；系统 MUST NOT 仅因端点相同而撤下其他实例。本条不豁免其他有效记录的必要引用检查；link ID SHALL 使用独立随机 128-bit 值，碰撞时重新生成，不覆盖旧实例。

#### Scenario: Create two links with identical endpoints deliberately
- **GIVEN** L1 已连接范围 A 到 B
- **WHEN** 用户在另一条命令中明确再创建方向和端点相同的关联，且检查通过
- **THEN** 新关联有不同的 link ID L2，L1 与 L2 都可区分
- **AND** 不把新操作静默合并到 L1，也不因端点已有关系而全局拒绝

#### Scenario: Reset withdraws only the link creation in the removed segment
- **GIVEN** L2 在范围 B 的 b0 时已有效，随后 B 的 ATOMIC 块 s 到 e 内首次创建同端点的 L1，且没有其他必要依据变化
- **WHEN** 用户成功执行 `reset <s>` 落在 b0
- **THEN** L1 的创建记录退出有效历史，当前有效关联仍含 L2、不再含 L1
- **AND** L1 的创建记录及 link ID 仍可从保留的历史中查到，不因查询重新生效

### Requirement: Links require an explicit user operation

系统 SHALL 只按用户显式 link 操作或显式关联选项创建关联实例。理由或 note 提及某个范围或 commit、自然语言声称适配，MUST NOT 被解释为创建 link 的授权。保持既有 range 关联 SHALL 不被误当作隐式创建新 link。适配已有 link 与创建另一个 link SHALL 保持可区分。

#### Scenario: A reason does not create another relationship
- **GIVEN** L1 连接范围 A 到 B，范围 X 尚未与 B 建立关联
- **WHEN** 用户提交 B 的新版本，理由提及 X，但没有执行 link 或指定创建关联的选项
- **THEN** L1 仍连接 A 到 B，不因提交了新版本而重建
- **AND** 系统不从理由创建 X 到 B 的 link

### Requirement: Adaptation identifies the link and selected changes with a reason

适配操作 SHALL 显式指定被处理的 link ID，并提供与该 link 对应的理由，同时记录选定的一个、多个或全部上游变化及版本依据。即使只有一条候选 link，也 MUST NOT 省略其 ID 或仅凭端点、理由文本猜测。未选中的 link 或变化 SHALL 保持待处理；该提交对下游产生的影响 SHALL 另行处理。创建另一个同端点 link 或提交不相关内容 MUST NOT 自动处理旧 link 的责任。OMD SHALL 记录用户选择，但不证明适配的业务语义正确。`commit clean --no--reason` 的例外 MUST NOT 扩大到普通适配。适配记录退出有效历史后，系统 SHALL 停止将该记录计为当前处理证据，并按恢复点的其余有效依据重新判断相应 link 与变化；MUST NOT 因该记录仍可历史查询而继续抵消待处理责任，也不能按相同端点统一改变其他 link 的处理结果。

#### Scenario: One of two upstream changes is handled
- **GIVEN** 范围 B 同时有经 L_A 从 A、经 L_X 从 X 带来的未处理影响
- **WHEN** 用户提交 B 的适配，显式指定 L_A、对应理由和 A 的这次变化
- **THEN** L_A 上选定的影响被处理，L_X 上的责任保留
- **AND** 不要求回 A 再补一次 clean 来证明 B 已适配

#### Scenario: Same endpoints do not identify the same obligation
- **GIVEN** L1 和 L2 都从范围 A 指向 B，并各有待处理责任
- **WHEN** 用户适配时仅指定 L1、对应理由及选定变化
- **THEN** 仅处理 L1 上选中的责任，L2 的责任保留
- **AND** 记录能明确查到本次适配对应 L1，不根据相同端点合并两者

#### Scenario: Adaptation without a link ID is rejected
- **GIVEN** 只有 L1 连接 A 到 B，并有待处理责任
- **WHEN** 用户请求适配，提供理由但未显式指定 link ID
- **THEN** 系统拒绝该适配，不因只有一条候选而自动选择 L1

#### Scenario: Adaptation without a reason is rejected
- **WHEN** 用户请求适配并指定 L1，但未提供对应理由
- **THEN** 系统拒绝该适配，不用 note、推断文字或 clean 的无理由例外补齐

### Requirement: Clean is a source-side branch stop

`commit clean` SHALL 在变更源端阻断本次变化对所选出向分支的影响，而非为下游完成工作盖章。用户 SHALL 能选择部分或全部出向关联；阻断 MUST NOT 消除其他来源、其他分支或未来变化的责任。系统 SHALL 接受 `commit clean --no--reason` 显式省略理由；省略理由不跳过操作、范围选择或版本校验。

#### Scenario: Stop one branch while retaining another
- **GIVEN** A 的本次变化影响 B 和 C
- **WHEN** 用户在 A 上 clean 到 C 的分支
- **THEN** A 到 C 的本次责任被阻断，A 到 B 仍待处理
- **AND** 该 clean 不自动适用于 A 的下一次变化

#### Scenario: Explicitly omit the stop reason
- **WHEN** 用户用 `commit clean --no--reason` 选择一条待处理分支，且版本校验通过
- **THEN** 产生新的 clean commit 并明确记录无理由选择
- **AND** 不伪造理由、不扩大所选分支，也不把这次操作当作 skip

### Requirement: Unclean preserves separate obligations

用户 SHALL 能在内容没有变化时提交 unclean，引入必须处理的脏状态。多次 unclean SHALL 保留为独立提交并平面列出全部，按时间序展示；系统 MUST NOT 仅因内容 hash 相同而合并或忽略其责任。

#### Scenario: Two requests for review use identical content
- **GIVEN** 范围内容保持不变
- **WHEN** 用户先后提交两条 unclean
- **THEN** 两条提交具有不同 ID，并在未处理列表中分别展示
- **AND** 不能仅因内容未变而报告全部已处理

### Requirement: Cycles do not cause infinite traversal

范围之间的 link SHALL 允许成环。对于同一次责任传播或依据检查，系统 SHALL 终止重复遍历，不因成环无限执行，也不能将不同变更或不同 link ID 的责任误合并为一个“已访问”。普通内容变化的逐节点适配与必要依据的间接断裂诊断 SHALL 分别处理。

#### Scenario: Check a circular set of references
- **GIVEN** 合法 range 关联形成 A 到 B 到 C 到 A 的环
- **WHEN** 系统检查这组关系
- **THEN** 检查终止，不仅因为成环而拒绝 link
- **AND** 不重复生成同一项变化的无限处理责任

### Requirement: Atomic blocks group one range commit chain

系统 SHALL 提供 `omd commit atomic begin` 与 `omd commit atomic end`，通过同一个 range 提交链上的特殊开始/结束 commit 点界定 ATOMIC 块。同一条链上两者之间的 commit SHALL 自动归入该块，保留各自的操作、顺序与身份；系统 MUST NOT 要求逐次传入额外 `--atomic` 标识，也不能按整个存储或终端会话收纳其他 range 的提交。块内 link 指向其他 range MUST NOT 把对方的提交链并入本块。同一条链 SHALL 支持嵌套 begin/end；end SHALL 关闭最近尚未闭合的 begin，关闭内层 MUST NOT 冒充外层也已闭合。

#### Scenario: Membership follows one range chain
- **GIVEN** 范围 B 的提交链上已有 begin
- **WHEN** 用户沿 B 的这条链依次记录 b1、创建 L1（范围 A 到 B）的操作，并记录 end
- **THEN** b1 与这次 link 操作自动属于 B 的同一个已闭合块，无须逐条指定块 ID
- **AND** A 的版本链不因为是 L1 的端点而成为这个块的成员

#### Scenario: Closing an inner block leaves its outer block open
- **GIVEN** 同一范围的有效链为 `b0 -> begin s1 -> b1 -> begin s2 -> b2`
- **WHEN** 用户写入一次 end
- **THEN** 该 end 关闭内层 s2，s1 仍未闭合，verify 仍报告闭合失败
- **AND** b2 的已生效状态不因缺少外层 end 而被隐藏或撤回

### Requirement: Open atomic blocks advance current state but fail closure checks

块内每次成功 commit SHALL 推进该 range 的当前状态，不等待 end；系统 MUST NOT 将这些记录仅视为尚未生效的隐藏草稿。`verify` SHALL 检查 begin 是否有对应 end 闭合，未闭合时报告检查失败；写入 end SHALL 关闭块，而不是首次发布此前已推进的状态。闭合检查通过 MUST NOT 替代其他内容、适配及必要依据检查，也不代表另行执行的 check 已有完整覆盖；check 的覆盖百分比 MUST NOT 代替 verify 的闭合检查。verify MUST NOT 自行补 end；检查失败不是新的范围标记生命周期。某一步失败 SHALL 保留此前已成功生效的提交，MUST NOT 自动撤回整块、补 end 或重试；用户可显式继续该链或按合法边界 reset。单次写入的崩溃完整性见 design.md 的工程契约，不等同于 ATOMIC 整块回滚。

#### Scenario: The current range advances before end
- **GIVEN** B 的 b0 范围为 10～20，链上随后已有 begin 和把范围改为 10～30 的成功提交 b1，但没有 end
- **WHEN** 用户查看 B 的当前范围并运行 verify
- **THEN** 当前范围是 10～30，不是 10～20
- **AND** verify 报告块未闭合而失败，不自行补 end

#### Scenario: End closes an already advanced state
- **GIVEN** 上述未闭合块中的当前范围已经是 10～30，其他检查均满足
- **WHEN** 用户写入对应 end 后运行 verify
- **THEN** 闭合检查通过，当前范围仍是 10～30
- **AND** 不把此前 b1 描述为到 end 时才第一次生效

### Requirement: Ordinary atomic members cannot be reset independently

系统 SHALL 拒绝将 ATOMIC 块内普通成员 commit 直接指定为 reset 目标，保持当前状态不变；即使它已是当前版本，或块尚未闭合，也不得例外。begin/end SHALL 作为占位标记符 commit 处理：它们有块归属与闭合的结构作用，但不是独立的正文恢复状态。用户 SHALL 能在任意嵌套层级指定 begin 或已存在的 end；系统 MUST NOT 因该标记也处于外层块内而拒绝。对这类目标的 reset SHALL 撤下标记及其后续记录，实际落在同链直接前驱，使撤下记录成为 dangling。这个 `-1` SHALL 只移动一次，即使前驱也是标记，也不能继续跳过或改退外层起点。块外普通目标仍保留本身；原记录和 note 保留，来源文件不变。不能仅以正文为空或未变化把其他有实际状态作用的 commit 归为占位标记。

执行边界 reset 时，系统 SHALL 额外发出 warning，明确请求的边界 ID 与实际前驱落点，JSON 输出也 SHALL 提供该警告。实际落点仍 SHALL 满足必要依据、版本及其他写入检查；无法完成时 SHALL 报错而不假称成功。对内部普通目标的拒绝 SHALL 给出真实边界 ID；end 尚不存在时 SHALL 说明，不编造它，也不自动替用户执行另一个 reset。禁止直接指定内部普通点，与边界规则可能落在块内前驱上 SHALL 分别判断。文件 reset 的限制见下方文件回退要求。

#### Scenario: Reset targets a commit inside a closed block
- **GIVEN** s 与 e 是某个已完成块的 begin/end 提交点，b2 是块内的范围提交
- **WHEN** 用户试图单独 reset 到 b2
- **THEN** 系统拒绝操作，错误指出 b2 在块内部并列出边界 ID s 与 e
- **AND** 不改变当前记录，不自动 reset 到任意边界

#### Scenario: An open block does not make its current interior commit resettable
- **GIVEN** s 是 begin 提交点，内部 b1 已将当前范围推进到 10～30，且尚无 end
- **WHEN** 用户试图 reset 到 b1
- **THEN** 系统仍拒绝内部目标并保留当前状态，提示开始边界 s 及结束点尚未存在
- **AND** 不编造 end ID，也不自动回退到 s

#### Scenario: Reset begin withdraws the opening marker as well
- **GIVEN** 有效链是 `b0 -> begin s -> b1`，b0 的范围为 10～20、b1 为 10～30，尚无 end，b0 满足回退条件
- **AND** 同端点 L1/L2 在 b0 时均已存在且有待处理的 a1 责任，b1 仅显式适配 L1 的这次变化，L2 无其他变化或失效依据
- **WHEN** 用户执行 `reset <s>`
- **THEN** 实际停在 b0，当前范围恢复 10～20，s 与 b1 成为 dangling，仍可查看记录和 note
- **AND** L1/L2 仍是有效关联，L1 的该项适配随 b1 撤下而恢复待处理，L2 保持原来的待处理责任
- **AND** 提示 warning：请求的是特殊开始点 s，实际落点为其前驱 b0
- **AND** verify 不再因为已撤下的 s 报缺 end，也不自动追加 end

#### Scenario: Reset end reopens the block at its direct predecessor
- **GIVEN** 有效链是 `b0 -> begin s -> b1 -> end e`，b1 满足回退条件
- **AND** 同端点 L1/L2 在块前已存在且均有待处理的 a1 责任，b1 将范围改为 10～30 并仅显式适配 L1 的该项变化，L2 无其他变化或失效依据
- **WHEN** 用户执行 `reset <e>`
- **THEN** 实际停在 b1，e 成为 dangling，s 仍在当前有效链中
- **AND** 当前范围仍是 10～30，L1/L2 仍有效，L1 的该项责任保持已处理，L2 保持待处理
- **AND** 提示 warning：请求的是特殊结束点 e，实际落点为其前驱 b1
- **AND** verify 报块未闭合；这不意味着允许直接请求 `reset <b1>`

#### Scenario: A boundary reset warning is present in JSON
- **GIVEN** s 是有合法前驱 b0 的 ATOMIC begin 标记
- **WHEN** 用户以 JSON 输出执行成功的 `reset <s>`
- **THEN** 结构化结果包含特殊边界行为的 warning，能区分请求的 s 与实际落点 b0

#### Scenario: Nested boundaries use the same immediate predecessor rule
- **GIVEN** 有效链为 `b0 -> begin s1 -> b1 -> begin s2 -> b2 -> end e2 -> b3 -> end e1`，各实际落点满足其他回退条件
- **WHEN** 分别从这条完整链请求 reset s2 或 reset e2
- **THEN** reset s2 落在 b1，reset e2 落在 b2，各自撤下请求标记及其后续并发出 warning
- **AND** 不因目标处于 s1 内而拒绝，也不改退 b0；直接请求 reset b2 仍拒绝

#### Scenario: Adjacent markers are not skipped recursively
- **GIVEN** 有效链为 `b0 -> begin s1 -> begin s2`，s1 满足实际落点的其他检查
- **WHEN** 用户请求 reset s2
- **THEN** 撤下 s2，实际落点是 s1，不再减一到 b0
- **AND** warning 给出请求 s2 和实际 s1；verify 仍报告 s1 未闭合

#### Scenario: A first BEGIN can be reset to an empty chain
- **GIVEN** 新范围加 link 的组合以 BEGIN s 为该链首个 commit，s 没有前驱
- **WHEN** 用户请求 reset s，其他写入检查通过
- **THEN** 撤下 s 及后续，范围有效链为空并撤下挂载，返回 requested_id=s、actual_id=null 和 warning
- **AND** 保留已发布历史，不编造前驱，也不将该操作解释为 reset 虚拟根

### Requirement: Directional link options create an atomic combination

创建范围 B 的提交时，用户 SHALL 能显式使用 `--link-from <range>` 或 `--link-to <range>` 创建新的关联实例。from SHALL 创建参数所指范围到 B 的关联；to SHALL 创建 B 到参数所指范围的关联，每个实例有自己的 link ID。范围提交与所选 link 操作 SHALL 按顺序记录在 B 的同一个 ATOMIC 块中；成功完成该组合操作 SHALL 写入对应 end 闭合。各次成功提交的当前状态推进 SHALL 遵守上方未闭合块要求。该简写 SHALL 遵守与分别显式执行相同的范围、旧版本、必要引用和修复检查。该简写 MUST NOT 凭理由猜测关联、代替指定已有 link ID 的适配、自动处理未选中的责任或绕过 broken 修复。文中 `<range>` 及 A/B 等为端点范围引用示意，不冻结范围定位格式。

用户 SHALL 能在同一命令中同时使用两个方向的选项，也能重复指定各方向的不同范围。新建范围的组合 SHALL 以 BEGIN 为首提交；尚无正文成员时不贡献覆盖，reset 该无前驱 BEGIN SHALL 产生空链并撤下挂载、返回 actual_id=null。已有块内的组合 SHALL 打开自己的内层块，成功时只关闭自己打开的块。同一命令内，同方向选项重复定位到同一 range 时，系统 SHALL 报错并拒绝该命令的有效变更，MUST NOT 静默去重或建立两条相同关联。重复判定 SHALL 按方向及解析后的 range 身份，而非参数文本或某个版本的 commit ID；相反方向不属于重复。本条 MUST NOT 被扩展为跨命令或全局禁止创建同端点的其他 link 实例。

#### Scenario: Explicit incoming link to the updated range
- **GIVEN** A 是用户选定的范围，b1 是范围 B 本次提交的有效前驱
- **WHEN** 用户提交 b2 并显式指定 `--link-from A`，且相关检查通过
- **THEN** b2 与新的 A 到 B 的 link 操作记录在 B 的同一个已闭合块中
- **AND** 不额外创建 B 到 A 的反向 link

#### Scenario: Explicit outgoing link from the updated range
- **GIVEN** C 是用户选定的范围，b1 是范围 B 本次提交的有效前驱
- **WHEN** 用户提交 b2 并显式指定 `--link-to C`，且相关检查通过
- **THEN** b2 与新的 B 到 C 的 link 操作记录在 B 的同一个已闭合块中
- **AND** 不额外创建 C 到 B 的反向 link

#### Scenario: Mix directions and repeat each option for distinct ranges
- **GIVEN** A、X、C、D 是不同的有效 range，b1 是范围 B 本次提交的有效前驱
- **WHEN** 用户提交 b2，并指定 `--link-from A --link-from X --link-to C --link-to D`，且相关检查通过
- **THEN** b2 与创建 A 到 B、X 到 B、B 到 C、B 到 D 四个关联实例的操作依次记录在 B 的同一个已闭合块中，四个实例各有自己的 link ID

#### Scenario: Reject a repeated incoming range in one command
- **WHEN** 用户在同一命令中指定 `--link-from A --link-from A`
- **THEN** 系统报告同方向同范围重复，拒绝该命令的有效变更
- **AND** 不静默去重，不留下部分生效的范围提交或 link

#### Scenario: Reject a repeated outgoing range in one command
- **WHEN** 用户在同一命令中指定 `--link-to C --link-to C`
- **THEN** 系统报告同方向同范围重复，拒绝该命令的有效变更

#### Scenario: Different references to one range are still duplicates
- **GIVEN** 两个合法范围引用都定位到同一个 range A
- **WHEN** 用户在同一命令中用两个 `--link-from` 分别指定它们
- **THEN** 系统按同一 range 判定重复并报错，不因引用文本或版本不同而创建两个 link

#### Scenario: Opposite directions to one range are distinct
- **WHEN** 用户提交范围 B 的 b2，显式指定 `--link-from A --link-to A`，且相关检查通过
- **THEN** 系统在同一原子块中记录 b2，并创建 A 到 B、B 到 A 两个各有 link ID 的关联实例
- **AND** 不因端点相同而把两方向判为重复

#### Scenario: The duplicate check is scoped to one invocation
- **GIVEN** 较早的命令已通过 `--link-from A` 创建 A 到 B 的 L1
- **WHEN** 用户在另一条命令中提交范围 B 的新版本，并仅指定一次 `--link-from A`，且检查通过
- **THEN** 系统创建同方向同端点、ID 不同的 L2，不因已有 L1 而报告本次参数重复
- **AND** L2 的创建不自动处理 L1 的未完成适配责任

### Requirement: Range reset restores the selected commit point

对某 range 以不在 ATOMIC 块内部的普通 commit 为目标执行 `reset <commit-id>` 时，系统 SHALL 保留目标 commit、恢复它记录的范围状态，并使该跟踪目标之后的提交退出有效历史成为 dangling。ATOMIC begin/end 目标 SHALL 按上方特殊边界规则回退到其前驱并警告，不能沿用普通目标的保留语义。reset MUST NOT 修改来源文件、追加反向 commit、物理删除历史或 note。虚拟全局 root SHALL 不可 reset。文件提交的回退见下方独立要求，不能用本条的 range 自身回退场景代替。

#### Scenario: Undo a range extension
- **GIVEN** 同一 range 的 r0 记录 10～20，后续 r1 记录 10～30，且本次回退不拆开 ATOMIC 块
- **WHEN** 用户 reset 到 r0
- **THEN** 该跟踪恢复 r0 的 10～20 范围，r0 保留有效，r1 变成 dangling
- **AND** r1 及其 note 仍可查看，磁盘文件内容不变

#### Scenario: Create a new commit after range reset
- **GIVEN** 用户已从 r1 reset 到 r0
- **WHEN** 用户再次修改该 range 并提交
- **THEN** 创建以当前有效前驱为依据的新 commit，使用新 ID
- **AND** 不改写或复用旧 r1 的 ID

### Requirement: File reset restores the ranges at the target commit

对文件执行 `reset <文件 commit-id>` 时，系统 SHALL 先检查目标所记录的全部子范围版本；若任一子范围的恢复目标是块内普通成员 commit，SHALL 拒绝整次文件回退，保持文件与所有子范围不变，并报告阻塞范围、记录的提交和真实块边界，MUST NOT 擅自改选其他落点。对满足回退约束的目标，系统 SHALL 保留目标文件 commit，并一并恢复该文件在目标提交时记录的范围版本，无须用户逐个 reset 范围。被撤下的后续文件/范围提交 SHALL 退出当前有效历史成为 dangling，但记录与 note SHALL 保留可查。reset MUST NOT 修改磁盘来源文件，也不能把保留目标下的全部范围一律清空。本条的恢复映射 SHALL 由文件目标记录的 `range_id -> tip` 保存，不按时间戳或当前范围表猜测历史。恢复文件记录中的子版本不等于逐项调用独立的 range reset：合法目标中的边界版本 SHALL 精确恢复，不额外再退一步；普通内部成员仍按上方规则整次拒绝。

#### Scenario: Reset the file without separately resetting its range
- **GIVEN** 文件 B1 提交时范围 r0 为 10～20，后来范围另提交为 r1（10～30），文件也提交了 B2，且本次回退不拆开 ATOMIC 块
- **WHEN** 用户执行 `reset <B1>`，未单独 reset 范围
- **THEN** 文件有效记录回到 B1，范围有效记录回到 r0（10～20）
- **AND** B2 与被撤下的 r1 成为 dangling，仍能查看其记录与 note
- **AND** 磁盘文件内容保持不变

#### Scenario: A file target inside a child block rejects the whole reset
- **GIVEN** 文件 F1 记录子范围 B 的版本 b1，b1 位于 begin s 与 end e 之间且不是边界；另一个子范围 C 的目标合法
- **WHEN** 用户从之后的文件状态请求 reset F1
- **THEN** 拒绝整次请求，文件、B、C 均保持操作前状态
- **AND** 报告 B、b1 及实际存在的边界，不自动将 B 改退 s 的前驱，不先恢复 C

#### Scenario: A file snapshot preserves a recorded child END
- **GIVEN** 合法文件目标 F 记录子范围当前版本为已闭合块的 END e
- **WHEN** 用户 reset 文件到 F
- **THEN** 子范围恢复到记录的 e，保持该闭合状态，不把恢复映射改成对 e 的独立 reset
- **AND** 用户直接请求 range reset e 时仍应撤下 e 并落到其直接前驱

### Requirement: Broken required references are diagnosed transitively

必要依据直接或间接引用 dangling commit 时，系统 SHALL 报告 unreachable_link，展示断裂路径，且不得报告该检查通过。它 SHALL 给出经核对依据完整的可回退 ID；无合格目标时明确说明。用户 SHALL 先 reset 到依据完整的提交，再手工重建；普通 commit、clean、unclean 或 note MUST NOT 用于绕过此修复。

#### Scenario: Indirect breakage is visible before an intermediate reset
- **GIVEN** C 的 c1 必须依赖 B 的 b1，b1 必须依赖 A 的 a1
- **WHEN** A reset 后 a1 变成 dangling，而 b1 本身仍可从有效根找到
- **THEN** B 和 C 均报告 unreachable_link，C 的诊断包含 `c1 -> b1 -> a1`
- **AND** 不等到 B reset 才诊断 C，不自动将 b1 变成 dangling

#### Scenario: Reading data does not repair its validity
- **GIVEN** b1 引用的数据 a1 仍在磁盘，但 a1 已 dangling
- **WHEN** 用户查看 a1 或给 b1 添加说明
- **THEN** 数据可供解释，但该行为不恢复 a1 的有效性，也不修复 b1

### Requirement: Commit identity preserves original inputs

每条新 commit SHALL 记录时间，并按 `salt_16_chars + previous_id + timestamp + content + operation_payload` 的字段顺序形成 SHA-256 输入；盐为第一字段，同节点首条提交的 previous_id 为 `""`。完整不可变操作输入 SHALL 参与 ID；project_id、note、缓存与派生状态 MUST NOT 参与。已有提交被复制或引用时 SHALL 保留原输入和 ID；新操作 SHALL 生成新盐和新 ID，不能覆盖旧提交。

#### Scenario: Copying a record does not change its identity
- **GIVEN** 一个 commit 的原始盐、前驱、时间、内容和操作输入已保存
- **WHEN** 该记录被复制到另一个位置供本地项目引用
- **THEN** 原 ID 保持不变
- **AND** 不把新的项目身份或绝对目录注入旧 commit 的 hash 输入

### Requirement: Explicit timestamps support manual replay without automatic merge

commit SHALL 接受 `--timestamp <RFC3339>` 显式指定提交时间，规范化为 UTC 纳秒表示后参与该次新 commit 的 ID，不要求该值接近当前时间。系统 MUST NOT 因指定时间戳而绕过单写者锁、版本冲突拒绝或有效前驱要求。OMD SHALL 不提供自动合并或冲突解决工具；用户查看历史后手工重做，不恢复旧 ID 冒充原提交。

#### Scenario: A replay time does not bypass a version conflict
- **GIVEN** 用户选择历史记录的时间准备手工重建，但本次依据版本已过期
- **WHEN** 用户带指定时间提交
- **THEN** 系统仍拒绝冲突写入
- **AND** 不按时间戳替用户选择哪条分歧历史获胜

### Requirement: Notes are separate append-only records

note SHALL 独立持久化，使用独立随机 128-bit ID，保存 `id, timestamp, commit_id, kind, target_note_id, text`；不增加 author、回复树或审批字段。修改/删除 SHALL 追加指向原 note ID 的 patch/delete 记录，不原地覆盖原始 note；发布状态 SHALL 保留追加顺序，不能按可变时钟决定修订先后。任意保留 commit SHALL 可查询 note 列表。note MUST NOT 改变 commit ID 或传播结果，也不能只保存在可丢弃缓存中。

#### Scenario: Correct a comment without rewriting a commit
- **GIVEN** commit c1 附有 note n1
- **WHEN** 用户修改 n1 的说明
- **THEN** 新增具有自己 ID 的 patch note，指向 n1，原始记录保留
- **AND** c1 的 ID、理由输入和责任处理状态不变

#### Scenario: Note revisions follow publication rather than wall-clock order
- **GIVEN** n1 后依次发布修订 n2 和 n3，但系统时间回退使 n3 的时间早于 n2
- **WHEN** 读取 note 的当前说明及历史
- **THEN** 按持久追加顺序应用修订，n3 不因时间较早而失效，所有原记录仍可检查

### Requirement: Garbage collection is explicit and protects referenced records

系统 SHALL 仅在用户显式请求 gc 时清理提交，不随 reset、verify 或查询自动清理。可清理的 commit SHALL 同时从有效虚拟根不可达且无有效 commit 引用；被有效记录引用的 dangling SHALL 保留。清理一个 commit 时 SHALL 一并清理其 note，不删除其他有效记录的必要数据。

#### Scenario: Keep evidence of a broken reference
- **GIVEN** a1 已 dangling，当前有效的 b1 仍引用 a1
- **WHEN** 用户运行显式 gc
- **THEN** a1 及其 note 保留，便于断链诊断与查看

#### Scenario: Collect an unreferenced dangling record
- **GIVEN** a1 已 dangling，且没有有效 commit 引用它
- **WHEN** 用户显式清理该记录，写入版本检查通过
- **THEN** a1 及其 note 可被清理
- **AND** 其他有效记录及其必要来源内容不受损失

### Requirement: History and tree queries do not mutate validity

`omd list --dangling` SHALL 列举当前所选存储中的悬空记录；`omd log <commit-id>` SHALL 允许查看保留的提交历史，包括 dangling 和 tombstone。`omd tree` SHALL 默认从虚拟根展示挂载层级到 range，支持指定起始 commit 及 project/file 层级限制。tree SHALL 不把 range link 当作挂载边。上述查询 MUST NOT 恢复提交、执行来源命令或清理数据。无参数 log SHALL 列出当前所选存储的保留提交，按时间倒序、同时间按 ID 稳定排序；排序不是前驱关系。

#### Scenario: Inspect a dangling commit
- **GIVEN** r1 的记录仍存在但已经 dangling
- **WHEN** 用户按其 ID 查询 log
- **THEN** 可查看其历史信息
- **AND** r1 仍保持 dangling，不因查看而重放或恢复

#### Scenario: Limit a tree to file level
- **WHEN** 用户从虚拟根请求只显示到 file 的 tree
- **THEN** 输出相应的项目与文件层级，不展开 range
- **AND** range 之间的 link 不产生额外挂载子树或循环

### Requirement: JSON and explicit skipping preserve meaning

所有命令 SHALL 支持 `--json` 供脚本集成，结构化结果 SHALL 能区分数据、诊断和失败，不把失败包装为通过。用户显式跳过或关闭检查用例 SHALL 被报告，且不改变空/脏/已确认状态、不修复断链、不赋予确认资格。人和 AI SHALL 使用同一操作与检查规则。

#### Scenario: Skipping a failed check does not confirm a range
- **GIVEN** 一个范围处于脏态
- **WHEN** 用户显式跳过对应检查并请求 JSON
- **THEN** 结果标明该检查被跳过，范围不因此成为已确认

#### Scenario: Tree is machine-readable
- **WHEN** 用户请求 `omd tree` 的 JSON 输出
- **THEN** 返回可解析的层级数据，不要求脚本从终端树形字符反推挂载关系

### Requirement: Canonical records preserve all immutable hash inputs

权威记录 SHALL 使用 design E-2 的 TOML 记录及完整原始内容布局；业务对象以所选存储中该链首个 commit ID 为身份，不将首个 ID 自引用放入 hash。来源版本 SHALL 在引用它的 commit 前生成独立 128-bit ID；相同来源定义、视图与内容可以复用版本，不合并新 unclean 等业务操作。不同版本可以共享完整内容文件。

SHA-256 输入 SHALL 为 16 个 OS CSPRNG 生成的 `[A-Za-z0-9]` ASCII 盐字符，之后依次对 previous_id、UTC RFC3339 纳秒 timestamp、完整来源原字节、JCS operation_payload 进行 `u64 大端字节长度 + 字节` 分帧。previous_id 首条为 `""`。schema、kind、content_ref 及全部不可变操作字段 SHALL 纳入 canonical payload；同名索引投影不一致、未知字段、重复键、越界值或缺必填字段 SHALL 被拒绝。JCS 使用 RFC 8785，大整数编码为十进制字符串，不归一化用户文字。BEGIN/END 没有新正文时可使用空 content，其必要依据仍由 payload 引用；不能据正文未变把其他业务操作当作占位。

project_id、本存储归属、当前登记版本、note、缓存和派生状态 MUST NOT 直接或借完整 manifest hash 进入业务 ID。跨存储引用的目标 store_id 是解析命名空间，与 project_id 区分；独立校验物理发布/登记凭据，不将其变成业务前驱。复算 SHALL 使用保存的原始输入与确切内容，不读取当前来源替代历史；mutable binding 不改变原始载荷。

#### Scenario: TOML formatting does not alter a commit ID
- **GIVEN** 原始逻辑字段和值不变，只有 TOML 键排布与空白改变
- **WHEN** 重新计算 ID
- **THEN** canonical 输入和 ID 相同；改变 schema、kind 或其他不可变字段则不能保留原 ID

#### Scenario: Framed inputs cannot be confused by concatenation
- **WHEN** 两份记录在相邻字段中的字节分配不同，但简单拼接看起来相同
- **THEN** 长度分帧使输入可区分，不能因缺分隔而产生同一输入

### Requirement: CLI selections distinguish versions links and changes

普通 `commit --id` SHALL 选择当前有效 range tip，不能挪作 link 选择。范围变更使用 --range；atomic begin/end 使用 --id 选链。适配 SHALL 使用可重复的 `--adapt '<JSON 对象>'`，每项包含 `link_id, changes, reason`，changes 明确列出变化 commit ID，空理由或未选变化拒绝。源端 clean SHALL 用可重复 --stop 对象选择 link_id 与 changes，并提供 --reason 或明确 --no--reason。--link-from/--link-to 只创建新 link，不代替适配。

相应写入 SHALL 携带 --expected 凭据；完整 ID 或所选上下文中的唯一前缀均可使用，跨存储上下文由 --store 明确，不猜同名目标。可读历史不自动成为合法写入前驱，不以选择参数绕过 broken 修复。

#### Scenario: A selected link does not implicitly select all its changes
- **GIVEN** L1 有多项待办，L2 与其同端点
- **WHEN** 用户的 --adapt 仅明确选择 L1 的一项变化并给理由
- **THEN** 只处理该项，其他变化与 L2 保留；缺 changes 或 reason 的输入在发布前拒绝

### Requirement: JSON output and exit codes expose actual outcomes

JSON SHALL 使用 `{schema_version, ok, data, diagnostics}` envelope；诊断包含 kind、severity、message、store、node、commit_id，未适用上下文使用 null，ID/大整数使用字符串。检查项 SHALL 区分 pass/fail/skipped/incomplete，不把这些当成范围标记状态。覆盖 SHALL 给出来源、单位、covered/total、百分比、gap 坐标；当前来源不可取得时为 incomplete、百分比 null，不能假造合法空内容。

边界 reset SHALL 输出 requested_id、actual_id 和 warning；组合失败 SHALL 输出已成功 ID、失败步骤、开放边界和 operation ID。发布结果不确定不得伪称零变更。退出码 SHALL 为 0 成功（单纯 warn 不失败）、1 检查失败或不完整、2 用法/格式错误、3 版本冲突、4 锁冲突、5 I/O/执行失败。JSON 仅写 stdout，进度及来源 stderr 不混入 JSON；业务发布成功但缓存更新失败可返回成功并给 warning。

#### Scenario: Unknown current output is not empty successful coverage
- **WHEN** check 没有取得当前 command 输出并请求 JSON
- **THEN** 该检查不完整且百分比为 null，不返回 total=0 的 100% 成功覆盖

#### Scenario: A combination reports earlier successful members
- **GIVEN** begin 与一个成员已经发布，下一步写入失败
- **WHEN** 命令以 JSON 返回失败
- **THEN** 输出实际成功 ID、失败步骤、未闭合边界与 operation ID，不声称整块已撤回

### Requirement: Cosmetic batch finishing reclassifies under the write lock

用户 SHALL 能以 `commit cosmetic <path>` 对该文件当前被过滤为结构无变化的脏范围执行批量收尾。该命令 SHALL 在取得新的调用方观察凭据并进入写锁后，对**锁内当前内容重新执行结构分类**；分类结果与用户意图确认时的过滤视图不一致的范围 MUST NOT 被续改，SHALL 如实留在报告中。收尾 MUST NOT 复用上一次 check 的分类结果作为写入依据。

通过重分类的范围 SHALL 逐条续改到新坐标，在一个 ATOMIC 块内发布；晚期 I/O 失败 SHALL 按既有部分发布契约如实报告成功成员、失败步骤、开放边界和 operation ID。分类证据（工具身份与版本、旧/新版本依据、判定）SHALL 随确认提交记录；收尾 SHALL NOT 引入第四种标记状态，确认后的范围仍是普通已确认三态。下游 link 因收尾产生的待适配责任 SHALL 保留为独立待处理项，不被批量消除。

#### Scenario: Content changed between filtering and finishing is refused
- **GIVEN** 用户以 `--difftastic` 过滤后，某同事又向同一文件加入了逻辑修改
- **WHEN** 用户运行 commit cosmetic
- **THEN** 锁内重分类发现该文件不再是结构无变化，拒绝续改并如实报告
- **AND** 不基于过期的分类视图发布任何确认

#### Scenario: A formatting storm is finished in one command
- **GIVEN** 十个范围被过滤为结构无变化，用户已逐条审完 dirty 中的真实修改
- **WHEN** 用户运行 commit cosmetic 并通过版本校验
- **THEN** 十个范围在锁下重分类后于一个 ATOMIC 块内逐条续改，证据随提交落库
- **AND** 无过滤 verify 此后返回 exit 0，下游待办如实保留

#### Scenario: Partial publication is reported honestly
- **GIVEN** 批量收尾在第七个范围发布后遭遇 I/O 失败
- **WHEN** 命令以 JSON 返回失败
- **THEN** 输出已成功的成员、失败步骤、开放边界与 operation ID
- **AND** 不假称整块已回滚，也不把失败伪装成全部完成
