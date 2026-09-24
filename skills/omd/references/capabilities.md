# 功能分支与实践状态

本文件是操作路由，不是另一份规格。命令参数以所用二进制的 `--help` 为准；本仓库契约以活动 change 覆盖旧主规格为准，旧复合来源语法不用于新操作。

## 首批真实使用

初始化、import、text 范围、定向 link、JSON 查询、verify/check：按 [主流程](../SKILL.md) 操作。确切已完成步骤、范围与结果以 [自管理记录](../../../docs/omd-self-management.md) 为准；没有证据的步骤不因出现在本表而算通过。

## 按真实需求扩展（本轮尚未实践）

| 场景 | 操作与停止边界 |
| --- | --- |
| 修改后续改/适配/cosmetic 收尾 | 取得新观察，使用当前 tip 续改。`--adapt` 每项为 `{link_id, changes, reason}`；列明确切变化，不能处理一个 link 顺带清除同端点其他 link。`--difftastic` 过滤把 dirty 分桶后，`commit cosmetic <path>` 在锁内对凭据钉住的版本重分类并批量续改结构无变化的范围，证据落库；结构有变化或无法分类的仍走逐条续改。 |
| clean / unclean | clean 是源端按 link/变化停止传播；unclean 是独立责任，不是缓存或正文未变就能忽略。明确选择并说明理由，不批量消除待办。 |
| link 查询/健康检查 | `omd links` 概要（total/by_status/by_stratum），`links list` 分页明细（默认 20 上限 100，`--status/--node/--stratum` 过滤），`links show <id>` 单链完整投影。概要拒绝明细修饰参数；`--links node:<id>`/`status:<state>` 给 verify/check 嵌入一页明细。五态 + unchecked 分开计数。 |
| tags / named rules / skip | tag 分类；规则表达 spec→code 等关系及 warn/fail。按内容覆盖核对，不以对象数替代。显式 skip 只跳检查，不确认内容。先与用户确定规则方向和严格程度。 |
| rename / delete / remove / copy | 区分逻辑改名、工作区移动、tombstone、撤统计、新对象；不要把文件消失自动记成删除，不让新路径复用合并旧身份。 |
| ATOMIC / reset | 组合逐步发布，不是整体事务。开放块仍有当前状态但闭合检查失败；保留真实成功成员。reset 遵守直接前驱/内部成员边界；文件恢复保存的子范围版本。真实 reset 前确认影响。 |
| note / log / tree | note 是附加说明，不改原 commit hash；查询不修复 dangling，也不执行来源。历史可读不等于可作写入前驱。note 自身是链式对象（`note:<init-cid>`），修订序按链不按墙钟；旧 `notes/<id>.toml` 平文件格式直接拒绝，不迁移。 |
| audit | `omd audit add <seed> [--direction both|upstream|downstream] [--text]` 开一条 audit 链（默认 pending）；`audit show <id>` 按记录方向从种子走 link 图（both 时正反两个独立子图，不混），逐边 L0–L2、逐点活性；`audit list` 按 `--status pass|fail|pending`、`--start/--end`（audit 自身时间）、`--touched-start/--touched-end`（涂色端点版本时间）过滤并分页；`audit pass|fail|pending <id>` 追加结论 patch（append-only，不覆盖）。正文可写 `audit:<commit-id>` wiki 引用，解析钉住的 commit；Link 对象端点可指向 audit/note 链，双引用不一致报 mismatch。只有结构性 broken exit 1；fail/pending 是记录不是崩溃。`commit unclean --reason "audit:<id>"` 造脏索引，修复 commit 回链闭环。 |
| 编码 / byte | 显式编码、记录视图及配置有优先级，历史不随配置重解码。保存原字节，BOM/CRLF 计数；byte 不解码。 |
| command 来源 | 固定 executable + JSON 字符串数组；无隐式 shell/模板。init/replace 各自明确授权一次采集；verify/check 另需许可。只接受 exit 0 的完整 stdout，失败不能用部分输出或旧成功内容顶替；写入复用本次获准观察，不再执行。 |
| Git 历史 / replace | 完整固定 Git commit + 提交内路径，只读本地对象，无 fetch。当前仍观察登记文件。replace 核对完整内容，改变所选来源版本的恢复 binding，不改变对象或当前观察定义；同 hash 不等于同版本。 |
| 项目 alias / peer / remote identity | 共享身份与本机路径分开；必要 peer 先保护后发布，缺失/旧映射拒绝。remote 校验只在显式登记时使用，不自动等价化 URL、联网或 force。 |
| 移动 / activate | 同一权威移动保留 store ID；独立可写副本显式激活新 store ID，补齐外部保护，不自动重定向旧引用、不做同步合并。先在隔离环境实践。 |
| reindex / gc | 缓存可重建，不从当前来源替换历史，也不执行 command。GC 是显式破坏性操作，按保留引用闭包保护完整来源及 peer；先确认、先隔离验证。 |
| UML / Lean | 仅在实际采用时建范围 link。Lean 用于已有 UML 的顺序程序性质，先明确证明义务；不用于并发/锁证明。模型证明、实现对应、实现测试、OMD link 分别报告，不能用关联存在冒充语义证明。 |

## 在本仓库核对契约

- 当前字段、对象身份与写入边界：[活动 change](../../../openspec/changes/separate-range-identity-and-location/design.md)。
- difftastic 过滤、cosmetic 收尾与证据：[difftastic-cosmetic-filter](../../../openspec/changes/difftastic-cosmetic-filter/design.md)。
- import、范围、来源、恢复：[managed-content-tracking](../../../openspec/specs/managed-content-tracking/spec.md)。
- 适配、clean/unclean、ATOMIC、reset、note、GC：[change-review](../../../openspec/specs/change-review/spec.md)。
- 标签规则、跨项目与副本：[local-project-links](../../../openspec/specs/local-project-links/spec.md)。
- 程序采集与许可：[command-verification](../../../openspec/specs/command-verification/spec.md)。
- Lean 专用边界：[programming-thinking](../../../docs/programming-thinking.md)。

这些仓库链接不属于独立分发时的必需运行文件。移植 skill 到其他项目时，改为该项目实际规则入口；不要照搬 OMD 仓库的对象 ID、目录范围或已选工具。
