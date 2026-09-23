# 实施交接

## 当前状态

仓库已从设计交接进入 Rust 实现阶段：存在 `omd` CLI、权威文本存储、可重建缓存、file/command/git 来源、对象/link/跨 store 生命周期，以及 Rust/CLI/BDD 测试。

当前活动 change 是 [`separate-range-identity-and-location`](../openspec/changes/separate-range-identity-and-location/)。其规划已完成，独立审查及父侧核对后任务文件为 **23/24**，仅 5.1 保持未勾选。当时独立审查未发现需继续修复的缺陷；但 5.1 要求的原始失败证据已丢失，**整个 change 的完整验收为 BLOCK（历史证据缺口）**，不宣称 24/24。此后真实自管理中，经所有者授权新增文件 import/独立统计定位并修复筛选更新；最新回归与未完成检查见 [自管理记录](omd-self-management.md) 和 [补充证据](../spec-traceability.md#self-management-follow-up-after-the-24-task-review)，不沿用旧冻结候选的验收结论。

尚未交付：

- 完成全部功能实践的 OMD skills（已有 [仓库内首版](../skills/omd/SKILL.md)，未全局安装）；
- Lean 工程、证明或真实 UML↔Lean↔实现范围 link；
- 发布版本、hooks、许可证；
- Windows/macOS 全平台验证；
- 自动同步/合并多份可写 store。

## 当前接口摘要

- 范围身份是所选 store 中对象链的首个 commit ID；路径、位置、tip、有效范围版本和来源版本分别表达。
- 范围输入：独立 `<path>`、`--range <start> <end>`、`--mode text|byte`；续改使用当前 range tip 的 `--id`。
- 来源输入：`--source-type file|command|git` 与各自具名字段；旧复合来源、URI、`--source-ref`、`--source-json` 和路径拼坐标端点已被取代并拒绝。
- 本机 alias 映射保存在配置根 `projects.toml`；共享历史不保存个人绝对路径。
- 写入已有 store/object 前，调用方从 `verify --json` 取得 `data.expected` 并通过 `--expected` 提交。旧依据不自动刷新或重试。
- JSON schema version 2 分开报告对象、版本、位置、link、诊断和部分发布结果。

使用入口见双语 README；详细契约见 [来源与坐标](source-model.md)、[存储与路径](storage.md) 和 [任务证据表](../spec-traceability.md)。

## 已完成的组件收口

后续开发从 [设计与实现边界](../openspec/changes/separate-range-identity-and-location/design.md)、[任务及未完成项](../openspec/changes/separate-range-identity-and-location/tasks.md) 和 [逐项测试证据](../spec-traceability.md) 接手，不依赖执行流水账。以下仅概括已检查的组件范围，不代表整个 change 通过：

- S1：链身份、类型/挂载/位置投影、reset/tombstone/GC。
- S2：本机项目/实例映射、调用方观察凭据、remote identity。
- S3：peer/inbound 定位、先保护后发布、只读副本与 activate。
- S4：封闭来源字段、command/Git/file 采集、编码与 replace binding。
- S5a：结构化多端点、规范化重复预检、有序保护和真实部分发布。
- S5b：JSON v2、上下文诊断、text/byte 分单位覆盖和未知分母。
- S6 bootstrap：真正新项目仅含默认 `.omd/omd.toml` 时的配置先行 init。

最新 bootstrap 父门禁位于 `/home/xz/.cache/omd-parent-bootstrap-review-76limdcp/`：focused 6、full 383/29 groups、fmt/build/OpenSpec/diff-check 均 exit 0。它还独立证明 windows-1252、原始 `80 0d 0a`、完整 hash/长度、配置字节不变与 verify 成功。绿色总数本身不算任务证明；具体任务映射见 traceability 表。

S5b 固定结果位于 `/home/xz/.cache/omd-parent-s5b-fix-review-hy0oehr4/`，S5a 位于 `/home/xz/.cache/omd-parent-s5a-fix-review-uyomafgs/`。真实晚期 I/O 的精确成功 commit/link/protection 集合、失败 link 未发布、开放边界和 operation ID 已分别核对。

## S6 文档与 README 回放

双语 README 各自包含一段可执行的独立流程：

1. 初始化两个文件；
2. 每次写入前获取新的 caller evidence；
3. 从 JSON 读取真实 range ID；
4. 用该 ID 建 link；
5. 记录 rename 并移动工作文件；
6. 查询当前对象/link；
7. 修改需求并验证 dirty；
8. 执行 check。

S6 回放必须在不同的 HOME/`OMD_CONFIG_PATH`/`OMD_CACHE_PATH` fixture 中运行，并保存命令、退出码、返回 ID 和断言。README 只使用实际返回 ID，不把示例占位符当成提交参数。

## 证据边界

### stale 负例与 fresh 正例分开

- 永久测试 `observation_context::observed_existing_update_succeeds_and_stale_publication_fails_without_mutation` 先用 fresh evidence 成功发布，再用同一旧 evidence 得到 exit 3，并断言 metadata 字节不变。
- 来源在观察后改变的负例由 `file_change_after_observation_refuses_write_and_keeps_history`、`missing_source_observation_is_version_conflict` 等测试证明。
- 新鲜观察到的新内容成功发布由 `successful_changed_command_observation_commits_exact_bytes_without_rerun`、`changed_file_observation_reuses_exact_acquired_version_despite_unrelated_failure` 等测试证明。

历史 handoff 曾提到 `abcd -> abXYcd` 探针，但旧 `/tmp` 脚本/基线已丢失，无法确定其精确脚本与行号。该历史负例保留为未恢复的 provenance 限制，不计入通过证据，也不被新的 fresh 正例重命名或替代。

### `version record missing` 的正负证据分开

- 正例：`guards::effective_range_body_survives_link_end_and_two_renames` 证明 link/END/两次 rename 后仍用有效范围正文和来源版本，check/verify 成功。
- 负例：`guards::missing_effective_source_version_stays_negative` 实际删除必要 version 记录，verify exit 1 且仍报告 `version record missing`。

因此修复的是“结构 marker/link 不应伪造缺版本”；真正缺版本仍失败。OMD link 不证明语义等价。

### 部分发布事实

- 预检失败、stale、错误类型和锁冲突等路径分别断言零发布。
- late I/O 发生在部分成员已公开之后时，不承诺整体回滚。`exact-partial.json` 记录三个成功 commit、一个 link、一个 protection、精确开放边界和 operation ID。
- 失败 creator 的物理文件可能存在但未被 retained/published 选择；这不是发布成功，也不要求本次文档任务增加物理清理策略。
- 共享登记与本机配置不是跨文件系统分布式事务；报告必须区分共享权威和本机映射各自的实际发布结果。

## 剩余限制

- 旧 `/tmp` 修复前快照和若干原探针来源不可恢复；当前稳定树可用于精确 S6 差异，但不能声称恢复旧字节基线。
- 特殊字符行为有当前黑盒回归；原始 pre-fix `we@ird` 失败 artifact 不再可读，不能作为红绿 provenance。
- Windows 路径、argv 和持久化细节，以及 macOS 平台行为未由当前 Linux 门禁证明。
- 真正进程死亡/断电的全平台恢复没有端到端故障注入证明；现有 staged/post-rename/late-I/O 测试覆盖明确可注入边界。
- 产品 skills 与 Lean 证明未交付；README/文档不能宣称它们完成。
- 未进行 Git commit、push、release 或 OpenSpec archive。

## 最终审查与停止原因

本轮修复周期已消耗两轮审查：Codex 实际检查后因额度耗尽未出报告；Kimi K3 完成第 2 轮（workflow `8fb62d4e-c3da-413c-bdfc-72259d660b9f`）。其建议实现 OK、证据/合并 OK with notes，未发现新的必要修复。父侧接受当前行为结论，但不豁免 5.1 的历史 RED 要求，因此完整交付仍 BLOCK。

父侧复核并纠正报告口径：383 项 Rust 测试（29 result groups），另有 18 个 BDD scenarios / 79 steps，不能加成 480 项独立测试；README 所引 ID 在 S6 的两份 `evidence-summary.json` 中仍可核实，父侧另一次回放 ID 不同不构成证据丢失。

停止原因：只剩不能靠当前代码重跑补回的历史证据，以及已说明的可选/平台限制。不开第 3 轮、不制造新修复、不归档或发布。详见 [24 项任务证据表](../spec-traceability.md)。
