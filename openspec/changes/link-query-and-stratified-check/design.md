# Design

## Context

见 proposal.md——三个 JSON 出口（`list`、`verify`、`check`）无条件嵌入全量 `links`/`objects`；无 link 查询面；无分层错链检查。当前 `state.toml` 已有 `links`（link_id → 实例）、`link_pending`（义务）、`dirty`（脏状态与义务栈）、`tips`/`reset_from`（活性）、`open_blocks`；`identity_diagnostic` 已有结构诊断。

## Goals / Non-Goals

**Goals**
- 任何读命令的默认输出大小与 store 规模无关。
- link 查询面覆盖：概要 → 状态/节点/单链/层下钻 → 分页明细。
- 健康判定 L0–L2 早退，永不因查询触发内容读取或 command 执行。
- SCC 凝缩分层，环合法且同层；分层为派生只读视图。

**Non-Goals**
- 不改变脏传播、`clean --stop`、marker 三态——环下的传播终止沿用既有幂等语义（用户决定，零额外处理）。
- 不引入 link 健康的持久化缓存或第四种 marker；判定是查询时派生。
- 不做 L3 内容级验证下沉——内容真实仍由 `verify` 独立承担。
- 不提供自动拆环、环警告或环迁移工具。

## Decisions

### D-1 新动词 `omd links`，不扩展 `list`

`list` 的心智模型是状态转储，links 查询是"概要→下钻"两种心智。塞进 `list` 会让 flag 组合爆炸（status/node/id/stratum/limit/brief-full × dangling）。独立动词让分页契约只在一个地方实现。

替代方案：扩展现有 `list --links-*` 系 flag。拒绝原因：flag 空间线性增长，且 `list` 现有消费者（教程、skill）语义要保。

### D-2 五态 + unchecked 计数，状态判定每链早退

```
healthy / obliged / stale / withdrawn / broken   ← 互斥五态
unchecked                                         ← 计数，不进五态
```

判定顺序（每链早退，停在第一个失败层）：

| 层 | 检查 | 数据源 | 成本 |
|---|---|---|---|
| L0 | 记录可读、端点 key 合法 | state.links | O(1) |
| L1 | 端点 selected_version 仍有效、非 dangling | tips, reset_from | O(链长) |
| L2 | link_pending 非空 → obliged；持久化 dirty → stale | link_pending, dirty | O(pending) |

L2 内 obliged 优先于 stale（义务是更明确的"有事待办"）。`unchecked` 只在端点对应 command 来源且本次调用未获准运行时出现——判定它需要看 acquisition 类型，这是 L1.5 的只读操作，不执行。

替代：把 unchecked 做成第六态。拒绝：它不是链的属性，是本次调用的可判定性属性，混进状态机会让"昨天 healthy 今天 unchecked"看起来像链变了。

### D-3 游标 = `(filters, offset)` 提示性 token（用户已确认）

Token 编码过滤条件 + 偏移，重放时按当前 state 重新计算。State 变化导致偏移漂移时如实报告（`"note": "state changed since cursor"`），不拒绝、不静默修正。理由：OMD 哲学里写入凭据必须严格，但读游标是导航提示——拒绝陈旧游标会把一次浏览变成多轮重试。用户已确认此语义。

### D-4 SCC 凝缩分层，环合法同层（用户已确认）

Tarjan SCC O(V+E) 凝缩 → 凝缩 DAG 最长路径分层。根 = source 侧且非任何 link 的 target 的节点。环成员同层是诚实语义：环内对象互相依赖，检查时本来就只能一起看。`by_stratum` 只报计数；层内明细经 `--stratum <n>` 下钻。

跨 store peer 端点（`peer:<store>:<kind>:<root>`）作为不透明节点参与图计算，不解析、不要求远端可达。

替代：Kahn 拓扑序 + 环上任意报错。拒绝：环是用户显式合法状态（用户决定），报错违背契约。

### D-5 verify/check 嵌入改计数 + 显式过滤

```
data.links:    [ ...全量... ]           →  data.link_count: 432
data.objects:  [ ...全量... ]           →  data.object_count: 21
                                    可选 --links <filter> / --objects <filter>
                                    → data.links: { items, total, has_more, next }
```

`--links node:<id>` / `--links status:<state>` / `--objects path:<glob>` 复用 `omd links` 的过滤与分页实现（同一下钻函数）。dirty/cosmetic/规则/退出码不动。

### D-6 分层检查 = 概要 + 建议顺序，不是门禁

`omd links` 输出 `by_stratum` 与每层五态分布；工作流建议层号升序（上游修复会改变下游 pending 集，先查下游是浪费）。不强制：`--stratum` 只是查询过滤，没有"必须从 0 层开始"的检查。理由：修上游会级联改变下游，建议顺序最大化每次检查的信息量；但门禁化会把合法的自由探索变成流程负担。

### D-7 明细密度两档

`--brief`（默认）：`{link_id, status, source, target, stratum}`——五个字段，20 条 brief 有与 store 规模无关的上界。`--full`：现有 `link_projection` + status + stratum。没有第三档：密度梯度多一档，心智成本翻倍，两档覆盖"扫队列"和"看单链"两个真实场景。

## Risks / Trade-offs

- **BREAKING：JSON 消费者**：依赖 `data.links`/`data.objects` 全量数组的脚本要改读计数或走过滤参数。教程与 skill 中受影响命令逐一同步。迁移无自动层——旧字段语义变了，静默兼容（全量藏在计数后面）违背诚实报告。
- **L1 成本上界**：端点活性沿链走，病态长链（数千 commit）下单链判定退化到 O(链长)。可接受：这是既有 `log` 的成本量级，且 L0 失败时不触发。
- **分层不持久化**：每次查询重算 SCC。大图（万级 link）下单次查询成本可测。可接受：派生视图避免缓存失效这一整类问题；若实测成为热点，后续 change 再谈持久化层索引。
- **`stale` 判定精度**：L2 只读持久化脏状态，不重比内容。若上次 verify 之后内容又变了，`stale` 可能滞后。这是设计选择：查询永不触发内容读取，新鲜度由 `verify` 的调用频率决定，报告里不冒充实时。
- **state 内 schema_version**：state.toml 无版本字段，新增字段（如未来分页辅助）走 `#[serde(default)]` 既有惯例，不碰迁移。

## Open Questions

（无——游标语义、环合法性、分层算法均已在探索中与用户确认。）
