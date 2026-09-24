## MODIFIED Requirements

### Requirement: Link endpoints extend to audit and note commits with cross-reference consistency

Link 对象的端点类型 SHALL 从 range 扩展到 audit commit 与 note commit（本地对象 key 使用 `audit:`/`note:` 前缀加链上 commit id），link 的方向、实例身份、显式创建与适配语义 MUST NOT 因此改变。当 link 任一端点是 audit/note commit 时，检查面 SHALL 增加跨引用一致性：同一处关系若同时存在 Link 对象与载荷引用（钉 id 字段），两者 MUST 指向同一状态；不一致 SHALL 如实报告为检查失败项，不静默择一。端点活性判定沿用既有语义——钉住的端点 commit 被 reset 撤出有效历史时该端点判 withdrawn。

#### Scenario: A link to an audit commit is a first-class endpoint
- **GIVEN** 修复 commit c 通过 Link 对象连接 audit commit a2
- **WHEN** 用户查询 c 的关系或 audit show 走涂色区域
- **THEN** 该 link 按既有 link 语义参与（实例身份、方向、pending 不合并）
- **AND** a2 被 reset 撤走时该端点判 withdrawn，不判 broken

#### Scenario: Mixed reference mechanisms must agree
- **GIVEN** 修复 commit c 同时以 Link 对象（端点 a2）和载荷 reason（`audit:a3`）引用同一 audit
- **WHEN** 检查面评估该关系
- **THEN** 报告两机制指向不同状态（a2 ≠ a3）为一致性失败
- **AND** 不静默选择其中一个继续
