## MODIFIED Requirements

### Requirement: Canonical records preserve all immutable hash inputs

权威记录 SHALL 使用 TOML 记录及完整原始内容布局，按本 change 的结构化字段发布新 schema；业务对象以所选存储中该链首个 commit ID 为身份，不将首个 ID 自引用放入 hash。来源版本 SHALL 在引用它的 commit 前生成独立 128-bit ID；相同来源定义、视图与内容可以复用版本，不合并新 unclean 等业务操作。不同版本可以共享完整内容文件。

SHA-256 输入 SHALL 为 16 个 OS CSPRNG 生成的 `[A-Za-z0-9]` ASCII 盐字符，之后依次对 previous_id、UTC RFC3339 纳秒 timestamp、完整来源原字节、JCS operation_payload 进行 `u64 大端字节长度 + 字节` 分帧。previous_id 首条为 `""`。schema、kind、content_ref 及全部不可变操作字段 SHALL 纳入 canonical payload；同名索引投影不一致、未知字段、重复键、越界值或缺必填字段 SHALL 被拒绝。JCS 使用 RFC 8785，大整数编码为十进制字符串，不归一化用户文字。BEGIN/END 没有新正文时可使用空 content，其必要依据仍由 payload 引用；不能据正文未变把其他业务操作当作占位。

project_id、本存储归属、当前登记版本、note、缓存和派生状态 MUST NOT 直接或借完整 manifest hash 进入业务 ID。本机根目录、元数据目录及缓存 instance 定位也 MUST NOT 进入业务 ID。跨存储引用的目标 store_id 是解析命名空间，与 project_id 区分；独立校验物理发布/登记凭据，不将其变成业务前驱。复算 SHALL 使用保存的原始输入与确切内容，不读取当前来源替代历史；mutable binding 不改变原始载荷。

文件所属、范围位置、对象引用及来源描述 SHALL 使用具名字段，不再使用路径和坐标合成节点身份；首提交和后续提交均 SHALL 可在无缓存时按有效的同对象前驱关系恢复所属链。链首身份、当前 tip、有效范围版本和来源版本 SHALL 分别保存或确定性重建，索引投影不能替代原始依据。

#### Scenario: TOML formatting does not alter a commit ID
- **GIVEN** 原始逻辑字段和值不变，只有 TOML 键排布与空白改变
- **WHEN** 重新计算 ID
- **THEN** canonical 输入和 ID 相同；改变 schema、kind 或其他不可变字段则不能保留原 ID

#### Scenario: Framed inputs cannot be confused by concatenation
- **WHEN** 两份记录在相邻字段中的字节分配不同，但简单拼接看起来相同
- **THEN** 长度分帧使输入可区分，不能因缺分隔而产生同一输入

#### Scenario: Local mapping changes do not change canonical history
- **GIVEN** 原始记录、内容及逻辑定位不变，本机项目根从 `/work/app` 改到 `/home/team/app`
- **WHEN** 用户显式修正本机映射并复算已有 commit
- **THEN** 原 ID 保持不变，复算不使用新的绝对目录或当前文件内容

### Requirement: CLI selections distinguish versions links and changes

普通 `commit --id` SHALL 选择当前有效 range tip，不能挪作 link 选择。范围变更使用独立的 `--range <start> <end>` 和 `--mode text|byte`；atomic begin/end 使用 --id 选链。适配 SHALL 使用可重复的 `--adapt '<JSON 对象>'`，每项包含 `link_id, changes, reason`，changes 明确列出变化 commit ID，空理由或未选变化拒绝。源端 clean SHALL 用可重复 --stop 对象选择 link_id 与 changes，并提供 --reason 或明确 --no--reason。--link-from/--link-to 及其跨 store 形式只创建新 link，不代替适配。

相应写入 SHALL 携带 --expected 凭据；完整 ID 或所选上下文中的唯一前缀均可使用，本次存储由 --store 明确，跨存储端点的 alias 与 commit 按 structured-tracking-references 的每项参数组分别提供，不猜同名目标。可读历史不自动成为合法写入前驱，不以选择参数绕过 broken 修复。reset SHALL 使用专用 `--reset-target <commit>` 选择目标，不重载 --reason；边界与内部成员规则不变。

#### Scenario: A selected link does not implicitly select all its changes
- **GIVEN** L1 有多项待办，L2 与其同端点
- **WHEN** 用户的 --adapt 仅明确选择 L1 的一项变化并给理由
- **THEN** 只处理该项，其他变化与 L2 保留；缺 changes 或 reason 的输入在发布前拒绝

#### Scenario: A link ID cannot select the range being updated
- **WHEN** 用户将独立 link ID 传给普通 commit 的 --id
- **THEN** 请求被拒绝，不把它解释为适配或通过端点猜测待修改范围

### Requirement: JSON output and exit codes expose actual outcomes

JSON SHALL 使用 `{schema_version, ok, data, diagnostics}` envelope；诊断包含 kind、severity、message、store、node、commit_id，未适用上下文使用 null，ID/大整数使用字符串。node SHALL 是区分类型与链首身份的结构化对象，而不是必须解析的路径拼坐标字符串。范围数据 SHALL 分开报告 tip、有效范围版本、来源版本、所属文件和位置；link SHALL 分开报告 link_id、对象端点和版本依据。检查项 SHALL 区分 pass/fail/skipped/incomplete，不把这些当成范围标记状态。覆盖 SHALL 给出来源、单位、covered/total、百分比、gap 坐标；当前来源不可取得时为 incomplete、百分比 null，不能假造合法空内容。

边界 reset SHALL 输出 requested_id、actual_id 和 warning；组合失败 SHALL 输出已成功 ID、失败步骤、开放边界和 operation ID。发布结果不确定不得伪称零变更。退出码 SHALL 为 0 成功（单纯 warn 不失败）、1 检查失败或不完整、2 用法/格式错误、3 版本冲突、4 锁冲突、5 I/O/执行失败。组合操作 SHALL 保留根本错误分类，不能将输入解析或对象类型错误伪装为锁冲突。JSON 仅写 stdout，进度及来源 stderr 不混入 JSON；业务发布成功但缓存更新失败可返回成功并给 warning。

#### Scenario: Unknown current output is not empty successful coverage
- **WHEN** check 没有取得当前 command 输出并请求 JSON
- **THEN** 该检查不完整且百分比为 null，不返回 total=0 的 100% 成功覆盖

#### Scenario: A combination reports earlier successful members
- **GIVEN** begin 与一个成员已经发布，下一步写入失败
- **WHEN** 命令以 JSON 返回失败
- **THEN** 输出实际成功 ID、失败步骤、未闭合边界与 operation ID，不声称整块已撤回
