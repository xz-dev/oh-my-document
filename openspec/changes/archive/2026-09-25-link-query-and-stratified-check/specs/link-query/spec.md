## ADDED Requirements

### Requirement: The link query surface defaults to a summary, never a flood

`omd links`（无子命令）SHALL 只输出概要：`total`、按状态计数 `by_status`（含 `unchecked` 计数）、按层计数 `by_stratum`，不列出任何 link 明细。概要本身 MUST NOT 超过固定大小——它与项目规模无关。概要模式 MUST NOT 接受任何明细修饰参数（`--status`、`--node`、`--stratum`、`--limit`、`--cursor`、`--full`）；出现时 SHALL 报 usage 错误而不是静默忽略。

#### Scenario: A huge store still yields a bounded summary
- **GIVEN** 一个包含数千条 link 的大型 store
- **WHEN** 用户运行 `omd links --json`
- **THEN** 输出只有 total/by_status/by_stratum 计数，无明细数组
- **AND** 输出大小与 link 总数无关

#### Scenario: Summary rejects detail modifiers
- **WHEN** 用户运行 `omd links --limit 5` 或 `omd links --full`
- **THEN** 命令报 usage 错误并退出非零，不输出静默忽略这些参数的概要

#### Scenario: Summary counts include the unjudgeable
- **GIVEN** store 中存在 command 来源的 link 端点且本次未获准运行
- **WHEN** 用户运行 `omd links --json`
- **THEN** 概要中 `by_status.unchecked` 计数如实反映该数量
- **AND** 不把 unchecked 冒充为任何五态之一

### Requirement: Detail listing is an explicit action with a total-enumeration path

`omd links list` SHALL 输出分页明细；无过滤参数时 SHALL 枚举全部当前有效 link（分页游标遍历可达完整集合）。过滤参数 `--status <state>`、`--node <node-id>`（双向：source 或 target 匹配）、`--stratum <n>` 可组合使用。跨 store peer 端点 SHALL 按其持久端点对象 id 参与匹配，不解析为本地节点。明细响应 SHALL 包含 `total`（过滤后总数）、`has_more`、`next`（存在下一页时）与 `items` 数组。`--limit` 默认 20、上限 100；超过上限 MUST 截断并如实报告。游标 token SHALL 编码过滤条件与偏移，重放时按当前 state 重新计算；state 已变化导致偏移不再对应时 SHALL 如实报告漂移，MUST NOT 静默跳过或重复条目。游标是提示性偏移，不是写凭据，不参与任何写入许可判定。

#### Scenario: Unfiltered list enumerates everything in pages
- **GIVEN** 一个包含 57 条 link 的 store
- **WHEN** 用户运行 `omd links list --json`
- **THEN** 返回 20 条、`total: 57`、`has_more: true` 与游标
- **AND** 用户显式请求 `--limit 500` 时被截断到 100 并如实报告

#### Scenario: Cursor drift is reported honestly
- **GIVEN** 用户持有一页游标，期间其他调用者向 store 写入了新 link
- **WHEN** 用户重放该游标
- **THEN** 系统按新 state 重新计算过滤结果并给出对应偏移页
- **AND** 输出指出计数或偏移可能已漂移，不因游标过期而拒绝读取，也不把它当写入凭据

#### Scenario: Both directions of a node's links are listed
- **GIVEN** 节点 N 作为 source 出现在 3 条 link、作为 target 出现在 2 条 link
- **WHEN** 用户运行 `omd links list --node N`
- **THEN** `total: 5`，五条明细按分页返回
- **AND** 明细中每条的匹配端与方向可以从投影中区分

#### Scenario: Peer endpoints match by their persistent id
- **GIVEN** 一条 link 的 target 是跨 store peer 端点对象 P
- **WHEN** 用户运行 `omd links list --node P`
- **THEN** 该 link 出现在结果中
- **AND** 系统不尝试把 P 解析成本地节点或要求远端 store 可达

### Requirement: A single link shows full detail via an explicit show action

`omd links show <link-id>` SHALL 返回该 link 的完整投影（端点、钉住版本、创建 commit、状态、所在层），不附带任何其他 link 明细，MUST NOT 与过滤参数组合（组合时报 usage 错误）。未知 link id SHALL 报 usage 错误而非空成功。

#### Scenario: Show one link exactly
- **WHEN** 用户运行 `omd links show <link-id>`
- **THEN** 返回该 link 的完整投影
- **AND** 不附带任何其他 link 明细

#### Scenario: Show rejects filter combinations
- **WHEN** 用户运行 `omd links show <id> --status healthy`
- **THEN** 命令报 usage 错误，不静默忽略 `--status`

#### Scenario: Unknown id is an error
- **WHEN** 用户运行 `omd links show no-such-link`
- **THEN** 命令报 usage 错误并退出非零

### Requirement: Detail density has two explicit levels

`omd links list` 的明细 SHALL 有两档密度：`--brief`（默认）仅含 `link_id`、状态、端点对象 id 与层号；`--full` 含完整投影加健康判定细节（失败层、原因）。

#### Scenario: Brief is the safe default for agent context
- **WHEN** 用户运行 `omd links list` 且不指定密度
- **THEN** 每条明细只有 link_id、状态、端点对象 id、层号
- **AND** 20 条 brief 明细的总大小存在一个与 store 规模无关的上界
