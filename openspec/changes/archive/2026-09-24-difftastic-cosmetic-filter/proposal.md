## Why

一次格式化风暴（rustfmt、prettier 等）会让大量已跟踪范围同时变脏，而人和 Agent 的注意力其实只该落在真正的逻辑修改上。OMD 目前的 dirty 报告不区分"结构树有变化"和"纯格式变化"，两者挤在同一队列里，复查成本和遗漏风险都被放大。

## What Changes

- `verify` / `check` 新增 `--difftastic` 过滤模式：在既有 Myers 变脏检测之上，用壳调用 `difft` 二进制对旧/新完整内容做结构树比较。
- 过滤语义是**复查队列的优先级排序**，不是确认：
  - `data.dirty` 只保留结构树有变化的范围，**只有它决定 exit 1**；
  - 新增 `data.cosmetic` 低注意力桶（结构树无变化，被过滤）；
  - 无法分类（解析失败、byte 模式、工具缺失/失败、超大文件）保守留在 `dirty`，绝不静默归入 cosmetic。
- 过滤是**视图不是状态**：不带 `--difftastic` 的 verify 行为完全不变；范围在权威记录里仍然是脏的。
- 新增 `commit cosmetic <path>` 批量收尾命令：在写锁下**重新分类**后，把 cosmetic 集合逐条续改到新坐标，ATOMIC 块发布；分类证据（工具身份、版本、判定依据、旧/新版本 ID）随确认提交落库。
- 工作流定位：先逐条审 `dirty` 里的结构变化，最后批扫 `cosmetic` 收尾；完成定义仍是无过滤 verify exit 0。
- `--difftastic` 本身即本次执行的显式许可，与 `--run-command` 同一哲学：一次性、不携带到下一次调用、读取/查询命令永不触发。
- 同步更新 `skills/omd/SKILL.md`：教会 Agent 变更后**先过滤**、先审结构变化、最后收尾 cosmetic，并在工具不可用时诚实降级为全人工。
- 二期（不在本 change）：下游 `clean` 批量阻断、范围级分类。

## Capabilities

### New Capabilities

（无——本 change 修改既有行为，不引入全新能力域）

### Modified Capabilities

- `command-verification`: `--difftastic` 的外部工具许可模型——显式、一次性、壳调用外部二进制的执行边界与证据记录。
- `managed-content-tracking`: verify/check 的过滤语义——dirty/cosmetic/unclassified 分桶、退出码重定义、视图与状态的分离。
- `change-review`: `commit cosmetic` 写命令——锁下重分类、批量续改、ATOMIC 发布、证据落库、与既有三态和适配/clean 契约的关系。

## Impact

- `src/relations/`：新增 `DiffClassifier` 抽象（库层零重依赖），difftastic 适配器在 CLI 层。
- `src/main.rs`：verify/check 的 `--difftastic` 参数、报告分桶、退出码；`commit cosmetic` 命令。
- `src/records/`：确认提交的证据字段（closed payload 字段规则需要 schema 演进决定）。
- 运行时新增对外部 `difft` 二进制的依赖（仅在使用 `--difftastic` 时）。
- `skills/omd/SKILL.md`、`skills/omd/references/capabilities.md`：复核流程教学改写。
- 不改变持久格式版本、不迁移既有数据、不新增第四种标记状态。
