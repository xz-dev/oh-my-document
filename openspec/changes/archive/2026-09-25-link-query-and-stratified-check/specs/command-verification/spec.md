## MODIFIED Requirements

### Requirement: JSON reports bound their embedded link and object listings

`verify --json` 与 `check --json` 的 `data` 中，`links` 与 `objects` 字段 SHALL 不再是无界全量数组，改为：默认仅输出 `link_count` 与 `object_count` 计数；调用方通过 `--links <filter>` 与 `--objects <filter>` 显式请求明细时，按 link-query 分页契约返回过滤后的 `items`（含 `total`/`has_more`/`next`）。`data.dirty`、`data.cosmetic`、检查规则与退出码语义 MUST NOT 因此改变。

#### Scenario: A verify on a large store stays bounded
- **GIVEN** 一个包含数千条 link 的大型 store
- **WHEN** 用户运行 `omd verify --json`
- **THEN** 报告包含 `link_count` 与 `object_count` 计数，无全量明细数组
- **AND** dirty/cosmetic 报告与退出码与既有行为一致

#### Scenario: Filtered detail is available on demand
- **GIVEN** 用户正在排查某节点的验证失败
- **WHEN** 用户运行 `omd verify --json --links node:<node-id>`
- **THEN** 报告在 `links.items` 中返回该节点相关的 link 明细（分页契约）
- **AND** 不附带其他节点的 link 明细

#### Scenario: Existing exit-code semantics are unchanged
- **GIVEN** store 存在脏范围
- **WHEN** 用户运行 `omd verify --json`（无论是否带明细过滤）
- **THEN** 退出码仍由 dirty 决定，与嵌入明细的方式无关
