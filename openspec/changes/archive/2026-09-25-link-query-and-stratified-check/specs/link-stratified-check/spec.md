## ADDED Requirements

### Requirement: Link health is five states plus an unjudgeable count

系统 SHALL 对每条当前有效 link 判定五态之一：`healthy`（全部检查通过）、`obliged`（`link_pending` 非空——存在待 adapt 的下游义务）、`stale`（端点版本仍有效但关联范围内容已脏）、`withdrawn`（端点 `selected_version` 被重置撤走或创建 commit 不再处于有效历史）、`broken`（记录不可读、端点不是合法对象 key 等结构故障）。五态互斥；判定结果 SHALL 附带首次失败所在的层。command 来源端点因未获准运行而无法判定的 SHALL 计入 `unchecked`，MUST NOT 被归入任何五态——"无法判定"不是健康，也不是病。

#### Scenario: A pending obligation marks the link obliged
- **GIVEN** link L 的 target 提交了新版本，产生待 adapt 的 change-key
- **WHEN** 系统判定 L 的健康状态
- **THEN** L 报告 `obliged`，附带 pending 数量与所在层
- **AND** 不因存在义务而把端点标为 withdrawn 或 broken

#### Scenario: A reset endpoint is withdrawn, not broken
- **GIVEN** link L 的 source 端点钉住的版本 commit 已被 reset 撤出有效历史
- **WHEN** 系统判定 L 的健康状态
- **THEN** L 报告 `withdrawn`，指明被撤走的端点与撤出点
- **AND** 记录本身仍可读时 MUST NOT 报 broken

#### Scenario: An unrun command source is unchecked, never guessed
- **GIVEN** link L 的端点内容来自 command 来源且本次未获准执行
- **WHEN** 系统判定 L 的健康状态
- **THEN** L 计入 `unchecked`，输出说明无法判定的原因
- **AND** 不以历史成功观察冒充本次判定

### Requirement: Health checks exit early at the cheapest failing stratum

每条 link 的健康判定 SHALL 按成本升序执行：L0 结构完整性（仅读状态记录）→ L1 版本活性（端点钉住版本是否仍在有效链上、是否 dangling）→ L2 义务与脏状态（link_pending、DirtyState）。判定 SHALL 在第一个失败层早退并报告该层；L0–L2 均不读取源文件内容、不执行 command、不重新比较内容差异。内容级验证 SHALL 仍由既有 `verify` 独立承担；本查询面的 `stale` 判定 SHALL 复用持久化的脏状态，不重新计算。

#### Scenario: A structurally broken link never touches content
- **GIVEN** link L 的端点 key 不是合法对象标识（broken@L0）
- **WHEN** 用户查询健康判定
- **THEN** 判定在 L0 停止并报告 broken
- **AND** 全程未读取任何源文件内容或执行任何 command

#### Scenario: A clean link reports healthy without content access
- **GIVEN** store 中所有 link 均通过 L0–L2，且持久化脏状态为空
- **WHEN** 用户运行 `omd links --json`
- **THEN** 全部报告 healthy，`by_status.unchecked` 为 0
- **AND** 判定过程不需要读取任何源文件内容

### Requirement: Topological strata condense cycles into shared layers

系统 SHALL 以 link 的 source→target 方向构建有向图，对节点做强连通分量（SCC）凝缩，层号 SHALL 定义为凝缩 DAG 上从根（非任何 link 的 target 的 source 侧节点）出发的最长路径。用户显式创建的循环依赖 SHALL 是合法状态：环内所有成员 SHALL 获得相同层号（环 SCC 同层），MUST NOT 被拒绝、警告或要求拆环。`by_stratum` 概要 SHALL 如实计数每层 link 数；分层检查的建议顺序 SHALL 是层号升序（上游优先），但该顺序是建议性工作流，不是强制门禁。

#### Scenario: A deliberate cycle is legal and shares a stratum
- **GIVEN** 用户显式创建了 A→B→C→A 的循环依赖
- **WHEN** 系统计算拓扑分层
- **THEN** A、B、C 所在 SCC 凝缩为同一层，三条 link 的 by_stratum 计入同一层
- **AND** 系统不拒绝该状态、不输出要求拆环的警告

#### Scenario: Downstream of a cycle gets the next stratum
- **GIVEN** 上例的环上另有 D 依赖环成员（环→D）
- **WHEN** 系统计算分层
- **THEN** D 的层号严格大于环所在层
- **AND** 分层检查建议先处理环层再处理 D 层

#### Scenario: Dirty propagation through a cycle uses existing termination semantics
- **GIVEN** 环上某节点内容变脏
- **WHEN** 脏传播沿环扩散
- **THEN** 使用既有幂等标记与义务堆栈语义自然终止
- **AND** 用户仍以既有终止命令（如 clean --stop）显式收束，不需要新机制

### Requirement: Stratum computation is derived and bounded

分层 SHALL 是从当前有效 link 集合派生的只读视图，不持久化为权威状态；每次查询按当前 state 重新计算。计算 SHALL 在有限时间内终止（SCC 凝缩保证），不因环的存在而失败。

#### Scenario: Strata are recomputed, not stored
- **GIVEN** 用户重置掉一条 link 后再查询分层
- **WHEN** 运行 `omd links --json`
- **THEN** 分层反映 reset 后的有效 link 集合
- **AND** 不存在需要用户手动刷新的分层缓存

#### Scenario: Computation terminates on any graph
- **GIVEN** 任意结构的 link 图（含长环、自指、跨 store 端点）
- **WHEN** 系统计算分层
- **THEN** 计算终止并返回层号
- **AND** 跨 store peer 端点作为图的普通节点参与计算，不要求远端可达
