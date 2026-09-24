## ADDED Requirements

### Requirement: Difftastic filtering reorders the review queue without changing state

`--difftastic` SHALL 把 verify/check 的 dirty 报告重构为复查队列的优先级排序：`data.dirty` 只保留结构树有变化（或无法分类）的范围，**只有该桶决定失败退出码**；新增 `data.cosmetic` 桶列出被判定结构树无变化的范围。纯格式风暴 SHALL 产生 exit 0、dirty 为空、cosmetic 非空的结果，不再强制人注意纯格式变化。

过滤 SHALL 是**视图而非状态**：不带 `--difftastic` 的 verify/check 行为与现状完全一致，范围在权威记录里仍然是脏的，收尾前的完成定义仍以无过滤 verify 通过为准。无法分类（解析失败、byte 模式、超大文件、工具缺失或失败）SHALL 保守保留在 dirty，绝不静默归入 cosmetic。分类 SHALL 是文件级的：旧版本全文与新全文结构树比较；被过滤的范围集合 MUST NOT 被称为已确认或语义无变化，结构无变化是一个工具判定，不是语义证明。

#### Scenario: A formatting storm exits zero with a non-empty cosmetic bucket
- **GIVEN** 十个已确认范围全部只受 rustfmt 重排影响，结构树无变化
- **WHEN** 用户运行 verify --difftastic --json
- **THEN** `data.dirty` 为空、`data.cosmetic` 列出十个范围、命令 exit 0
- **AND** 同一时刻不带 `--difftastic` 的 verify 仍 exit 1 且 dirty 报告十个范围

#### Scenario: One logical edit survives the filter alongside formatting noise
- **GIVEN** 十二个脏范围中十一个纯格式、一个含常量 `3` 改 `5`
- **WHEN** 用户运行 verify --difftastic --json
- **THEN** `data.dirty` 只包含含逻辑修改的那个范围并 exit 1
- **AND** `data.cosmetic` 列出其余十一个，人/Agent 的注意力直接落在结构变化上

#### Scenario: Byte-mode and unparseable ranges stay in the review queue
- **GIVEN** 脏范围集合包含一个 byte 模式范围和一个 difftastic 无法解析其扩展名的范围
- **WHEN** 用户运行 verify --difftastic --json
- **THEN** 两个范围都保守保留在 `data.dirty` 中，不进入 cosmetic 桶
- **AND** 报告如实给出无法分类的原因，不以静默方式扩大过滤范围

#### Scenario: The filtered view is not a completion claim
- **GIVEN** 用户以 `--difftastic` 过滤后看到 exit 0
- **WHEN** 用户运行不带过滤的 verify
- **THEN** 命令仍按现有规则报告全部脏范围并 exit 1
- **AND** cosmetic 桶中任何范围在权威记录里都不是有效确认
