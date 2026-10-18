## Why

`omd list`、`verify --json`、`check --json` 目前无条件嵌入全量 `links` 与 `objects` 数组，且没有任何按节点或按 link 的查询入口：大型项目上一次读命令就会把 AI 的上下文塞爆，而逐链排错（错链定位、义务追踪、分层检查）只能靠外部工具在洪泛输出上手工过滤。需要一套"概要优先、游标下钻"的 link 查询面，以及一套从便宜到昂贵的分层错链检查方式。

## What Changes

- 新增 `omd links` 查询动词：默认输出概要计数（`total`、`by_status`、`by_stratum`），不列明细；通过 `--status` / `--node <id>` / `--id <link-id>` / `--stratum <n>` 下钻过滤；`--brief`（默认，id+状态）与 `--full`（完整投影）两档明细密度。
- 引入 link 健康状态判定：`healthy` / `obliged` / `stale` / `withdrawn` / `broken`（五态）外加 `unchecked` 计数（command 源未运行，无法判定，不进五态）。
- 引入拓扑分层（stratum）：以 link 端点构成的有向图上做 SCC 凝缩，层号 = 凝缩 DAG 从根出发的最长路径；环（用户显式创建的循环依赖）合法存在，环成员同层。`--stratum` 按层查询，分层检查建议上游优先。
- 分层错链检查语义：每条 link 的健康判定按成本从 L0（结构完整性，纯 state）→ L1（版本活性，tips/reset_from/dangling）→ L2（义务状态，link_pending/obligations）早退，报告停在第一个失败层；内容级验证（L3）仍由既有 `verify` 承担，`omd links` 不重新比较内容。
- 分页契约：所有列表响应带 `total`、`has_more`、`next`（默认 limit 20、上限 100）；游标 token 编码 `(filters, offset)`，重放时重新计算，state 已变时如实报告偏移漂移，不作为权威凭据。
- **洪泛修复**：`verify --json` / `check --json` 的 `links` 与 `objects` 字段从全量数组改为计数 + 过滤参数（`--links <filter>`、`--objects <filter>`）；`list` 改为概要 + 游标分页。BREAKING：依赖旧全量数组结构的 JSON 消费者需要改读分页字段。
- 脏传播与环处理零改动：用户显式创建的循环依赖合法；既有 `DirtyState.mark` 幂等终止、`commit clean --stop` 终止命令语义不变。

## Capabilities

### New Capabilities
- `link-query`: `omd links` 查询动词——概要计数、按状态/节点/单链/层下钻、brief/full 两档密度、分页与游标契约。
- `link-stratified-check`: link 健康五态 + `unchecked` 计数的判定边界，L0–L2 成本层早退语义，SCC 凝缩拓扑分层与环的合法地位。

### Modified Capabilities
- `command-verification`: `verify`/`check` 的 JSON 输出从全量 `links`/`objects` 数组改为计数 + 可选过滤明细（分页契约同上）。
- `managed-content-tracking`: `list` 从状态转储改为概要 + 游标分页。

## Impact

- `src/main.rs`：新增 `Cmd::Links`；`current_links`/`current_objects` 增加过滤与分页参数；`verify`/`check` 输出组装处（约 3414、3618 行附近）改计数。
- 新增 link 健康判定与 SCC 分层模块（`src/relations/` 下）。
- `tests/`：新增 links 查询与分层检查的集成测试；现有依赖全量 `links`/`objects` 数组的测试需适配分页字段。
- skill 文档（`skills/omd/`）与 README 双语运行边界需同步新查询面。
