## MODIFIED Requirements

### Requirement: State listing is a paged summary, not a full dump

`omd list` SHALL 输出概要（对象计数、link 计数、open block 计数）加游标分页的明细；`--dangling` 分支同样只输出计数与分页明细。全量 `tips`、`objects`、`links` 数组转储 SHALL 不再作为默认输出提供；需要完整导出的调用方 SHALL 通过分页游标遍历取得，结果 MUST 与遍历时的当前 state 一致。

#### Scenario: Listing a large store yields counts first
- **GIVEN** 一个大型 store
- **WHEN** 用户运行 `omd list --json`
- **THEN** 输出为计数概要与空明细（或首页游标），无全量数组
- **AND** 输出大小与 store 规模无关（不含明细时）

#### Scenario: Full export is achieved by cursor traversal
- **GIVEN** 调用方需要全部对象明细
- **WHEN** 其按游标逐页请求直至 `has_more: false`
- **THEN** 拼接结果覆盖当前有效集合
- **AND** 遍历期间的写入导致漂移时按游标契约如实报告
