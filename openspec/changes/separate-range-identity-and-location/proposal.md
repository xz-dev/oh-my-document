## Why

OMD 已约定业务对象以提交链身份持续跟踪，但当前实现又把路径、坐标模式和范围拼入节点 key 并反向解析，导致合法文件名被截断，也增加范围续改、改名和关联维护的一致性风险。项目尚无既有用户数据，应现在纠正模型，并让同一套记录能在不同本地目录的工作环境中解析，而不是增加分隔符补丁或旧格式兼容层。

## What Changes

- 保持既有身份契约：以所选存储中链的首个 commit ID 识别业务对象；同坐标可有多个独立范围，不另增 range UUID。当前 tip、有效范围版本、来源版本及 link ID 分别表达。
- **BREAKING**：删除 `range:<path>@<mode>:<span>[#nonce]` 身份与解析方式。路径、来源类型、坐标模式、起止位置改为独立字段；挂载、标签、关联、查询与覆盖率通过对象引用访问这些字段。
- **BREAKING**：范围创建／修改采用独立路径、`--range` 和 `--mode text|byte`；既有范围通过 commit ID 解析所属链。关联不再接受路径拼坐标端点；跨存储端点分别提供 store 与 commit。
- **BREAKING**：来源默认是文件，file/command/git 使用结构化来源字段和独立 CLI 参数。Git 的项目别名、仓库内路径、确切提交、可选 remote URL 分开；command 保留固定 executable 与 JSON 字符串 argv。取消旧复合 `--source-ref` 语法，不引入新的 URI 来源语法。避免不断扩充 scheme、query、组合及转义规则：人和 Agent 都提交明确参数，OMD 不再维护一套来源字符串语言；remote URL 只是独立的配置值。
- 复用项目／store 的稳定身份和已有 alias：共享记录保留逻辑项目身份与项目内相对位置，本机配置单独保存项目根／元数据目录映射。两位开发者的 checkout 绝对路径不同，不重算旧 commit，不改 link 身份。
- Git remote URL 属于显式身份约束，不是本机目录或自动获取指令。保留 D-29～D-31：Git 提供确切历史内容，verify 对真实文件仍读当前登记文件；replace 只允许同一完整内容。
- 发布明确的新记录格式；没有迁移、双格式读写或旧命令兼容层。遇到不支持的记录格式明确拒绝，不自动删除或重写。
- 用同坐标独立对象、范围续改、特殊字符路径、空正文 ATOMIC 标记、不同机器路径映射、跨 store 引用和缓存重建场景验证新模型；同步英文／中文 README 与 traceability。

## Capabilities

### New Capabilities

- `structured-tracking-references`: 对象引用、位置字段及版本证据的统一模型；来源参数拆分、范围端点解析、共享逻辑定位与本机路径边界。

### Modified Capabilities

以下沿用前序 `define-tracking-contracts` 的完整 capability 路径：

- `managed-content-tracking`: 明确范围位置与身份分离，来源描述结构化，改名／同内容 replace 不改变对象身份。
- `change-review`: 链身份索引、内容有效版本与 tip 分离，统一范围 commit 引用及结构化查询输出。
- `local-project-links`: alias 与本机映射分离，跨 store 引用拆字段，来源解析和可选 remote 核对不依赖个人绝对路径。
- `command-verification`: 将固定 executable 与 JSON argv 拆为独立来源字段，保留全部执行许可、工作目录及采集规则。

经用户明确授权，前序 `define-tracking-contracts` 的四份规格已同步到 `openspec/specs/`，作为本 change 的 MODIFIED 基线；Purpose、要求与场景均保留，仅转换主规格标题和 Requirements 章节格式。该同步不归档前序 change，也不把任务勾选或规格校验当作产品测试证据。本轮只完成基线同步及新 change 规划，不将本 change 的增量提前应用到主规格。

## Impact

- 规划实施范围：`src/relations/node.rs`、范围与来源模型、`src/records/` 的索引／提交／关联／恢复绑定、`src/main.rs` 参数及输出、本机项目登记／发现、相关 Rust 与 CLI/BDD 测试，以及双语 README。
- 保留提交 hash 的既有分帧和不可变输入规则、独立 link ID、三态标记、单写者与旧依据拒绝、显式 command 执行授权；不改为以 Git diff 判定内容变化。
- 不新增 URI 方案、迁移工具、自动 clone/fetch、跨机器同步或合并服务、分布式锁、依赖语言 AST 的定位算法、Lean 工程、许可证或发布动作。
- 资料依据：`docs/research/traceability-identities.md`；已确认业务语义主要来自前序 design D-17、D-24、D-29～D-33、E-1～E-5。URI 的中途讨论已被用户最新的“拆开字段并复用别名”决定取代。
- 本 change 只规划新的实现与验收；未执行产品修改、数据迁移、Git commit 或 push。
