## Purpose

为人和 Agent 提供同一套结构化跟踪引用：对象身份不取决于路径或坐标，来源、位置、版本依据及本机定位分别表达。允许多个工作环境使用各自的目录映射访问同一逻辑项目，不要求编写或解析 OMD 专用 URI。

## ADDED Requirements

### Requirement: Range positions are fields rather than object identities

系统 SHALL 分开表达所属文件对象、路径、来源类型、坐标模式、start 和 end。创建范围的 CLI SHALL 使用文件位置参数、独立的 `--range <start> <end>` 与 `--mode text|byte`；未指定模式的新范围默认 text，续改未指定模式时沿用该范围有效版本的模式。结构化记录 SHALL 保存具名字段，而非要求从路径拼坐标的字符串反推这些值。路径中的 `@`、`#`、`:`、空格、百分号或 Unicode 字符 SHALL 保持字面含义；各平台仍遵守自身合法文件名限制。OMD MUST NOT 对路径做 URI 解码或分隔符拆解。

范围身份 SHALL 是所选存储命名空间中该范围链首个 commit ID，不能由坐标、路径、当前 tip 或新增 range UUID 替代。选择既有范围 SHALL 使用 commit 引用，而非通过“路径加坐标”猜测其中一个对象；同坐标及重叠范围可以独立存在。

#### Scenario: A filename contains the old range delimiter
- **GIVEN** 当前平台存在文件 `we@ird #1%.md`
- **WHEN** 用户通过独立路径参数和 `--range 0 5 --mode text` 创建范围，再通过返回的 commit 引用关联它
- **THEN** 路径完整保留，关联指向该范围，不把 `@` 之前的部分当成另一个文件

#### Scenario: Identical positions do not select an arbitrary chain
- **GIVEN** 两个范围链首提交分别为 rA 和 rB，属于同一文件且都覆盖 text 的 0 到 5
- **WHEN** 用户通过 rA 所属链的合法引用续改或创建关联
- **THEN** 只选择 rA 所属对象，rB 的身份、历史和关联不被合并或重定向

### Requirement: Commit references resolve identity without weakening version checks

范围引用 SHALL 分开提供 store 上下文与 commit ID；本地引用默认使用本次所选存储，跨存储引用使用已登记 alias。完整 ID 或选定 store 内唯一前缀 SHALL 先解析到确切记录，再沿其同对象前驱确定链首身份。挂载父链、link 端点及来源版本 MUST NOT 被当成该链的前驱。必要记录缺失、前缀歧义、对象类型不符或错误 store SHALL 明确拒绝，不回退路径、同坐标对象或其他目录。

从保留 commit 得到链身份 MUST NOT 被解释为该 commit 当前有效、可写或已经复核。普通 `commit --id` 仍 SHALL 指定当前有效 range tip；必要依据、dangling、旧版本及锁检查不变。link 的端点对象 SHALL 已经存在并满足引用合法性；输入引用选择的版本依据 SHALL 保留，不偷偷替换成当前 tip。尚未存在的 commit 不得作为前向 link 占位。

#### Scenario: Two revisions identify one range but not one write precondition
- **GIVEN** r0 和 r1 属于同一范围，r1 是当前 tip
- **WHEN** 用户以 `commit --id r0` 请求普通续改
- **THEN** 系统辨认出同一范围，但拒绝过期写入，不自动改用 r1

#### Scenario: A file commit is not a range endpoint
- **WHEN** 用户把文件 init 的 ID 作为 link 范围端点
- **THEN** 输入被拒绝，不自动为该文件创建全文件范围，也不留下组合 BEGIN

#### Scenario: A commit is missing in the selected store
- **GIVEN** peer alias 已明确选定，但该存储没有请求的 commit
- **WHEN** 用户请求建立关联
- **THEN** 系统报告目标不可解析，不搜索同名路径或其他存储补齐

### Requirement: Chain identity tip and effective range version remain distinguishable

系统 SHALL 分别报告范围链首身份、当前 tip、当前有效范围版本及其完整来源版本依据。BEGIN/END 是结构标记，不要求携带新正文；当前 tip 是标记时，范围坐标与正文依据 SHALL 来自有效历史中适用的范围版本。clean、unclean、link 等有业务作用的记录不能仅因正文为空而被忽略。

新建范围的组合以 BEGIN 为首提交时，该 BEGIN 的 ID SHALL 是链首身份；正文尚未成功发布时有效范围版本为空，不贡献覆盖。后续正文和 END 不更换对象身份。reset 仍 SHALL 遵守直接前驱、内部普通成员拒绝、文件恢复映射及必要引用规则，不因本模型自动修复或补造依据。

#### Scenario: END is not mistaken for a missing content version
- **GIVEN** 范围链为首个 BEGIN s、正文提交 r、END e，完整来源版本 V 可取得
- **WHEN** 用户查看当前范围并执行 verify
- **THEN** 可区分对象身份 s、tip e、有效范围版本 r 和来源版本 V
- **AND** 不仅因 e 没有新正文就报告来源版本缺失，其他独立检查仍按实际状态判断

#### Scenario: Resetting the first marker does not create a new identity
- **GIVEN** 新范围组合的首个 BEGIN s 没有前驱
- **WHEN** 用户合法 reset s
- **THEN** 有效链为空并撤下挂载，返回 actual_id=null 与边界 warning，保留历史中的 s
- **AND** 不制造新 range ID；之后不带 --id 的新建操作仍创建独立新链

### Requirement: Source kinds use separate literal fields

来源 CLI SHALL 使用 `--source-type file|command|git`，未指定时默认 file；来源类型与 `--mode text|byte` 是不同维度。file 使用独立项目 alias 与路径，command 使用独立固定 executable 和 JSON 字符串数组，git 使用独立项目 alias、确切 Git commit 和仓库内路径；remote 名与 remote URL 是独立的可选身份约束信息。用户 SHALL 不必构造 OMD URI、`proj:`、`command::`、`git::` 或复合 source-ref 才能表达来源。

系统 SHALL 在采集或写入前拒绝缺必填字段、未知类型及不适用于所选类型的字段组合。独立 remote URL SHALL 只作为登记身份约束的值，不成为获取指令或可携带 path/range/command 的复合来源语言。普通路径字段中看似 scheme 的文字仍 SHALL 是路径文字，不触发类型切换或联网。

对既有对象的观察使用已登记来源，不因后续命令省略来源参数而重新解释为 file。历史恢复 binding 与当前观察 SHALL 分离；replace SHALL 保留同一完整内容及已有身份，不将历史替换变成当前来源切换。编码选择、原始字节保存及 command 许可规则不变。

#### Scenario: Byte coordinates do not imply a different source provider
- **WHEN** 用户对普通文件使用 `--range 0 8 --mode byte`，未指定来源类型
- **THEN** 系统仍读取 file 来源，坐标使用原字节，不推断为其他来源或解码文本

#### Scenario: Git history and current file have separate paths
- **GIVEN** 当前登记位置是 `docs/current.md`，指定 Git 历史版本内的位置是 `docs/old.md`
- **WHEN** 用户用独立 Git 字段登记确切旧版本，随后 verify
- **THEN** 历史从指定 commit 的 `docs/old.md` 获取，当前内容仍读取登记路径 `docs/current.md`
- **AND** 不使用 HEAD、remote 或历史路径替代当前观察

#### Scenario: Invalid source fields cannot launch a command
- **WHEN** 请求选择 file 类型却同时提供 command executable，或 command 的 argv 不是字符串数组
- **THEN** 在启动来源程序或发布业务记录前拒绝输入，原状态保持不变

### Requirement: Repeated directional endpoints have explicit store grouping

本地端点 SHALL 使用可重复的 `--link-from <commit>`、`--link-to <commit>`；跨存储端点 SHALL 使用可重复的 `--link-from-store <alias> <commit>`、`--link-to-store <alias> <commit>`，每次出现均固定绑定自己的两个参数，不使用平行数组配对或会影响后续参数的隐式 store 切换。结构化输入 SHALL 将每个端点表示为独立的 store 与 commit 字段。

四种选项 SHALL 能组合使用。每个引用解析后，系统 SHALL 在任何组合写入之前，按方向及规范化的 store/范围链身份检查本次输入重复；不同 alias、ID 前缀或同链不同版本不能绕过重复检查。不同方向不是重复，其他命令创建的同端点 link 也不是本次重复。成功创建的每条关系 SHALL 使用独立 link ID；适配仍显式选择 link ID 与理由。

#### Scenario: A command mixes local and peer endpoints
- **GIVEN** 本地 A、peer 项目 X 及本地 C 是可用的不同范围，B 是本次提交范围
- **WHEN** 用户提交 B 并指定 `--link-from A --link-from-store peer X --link-to C`
- **THEN** 创建 A 到 B、peer X 到 B、B 到 C 三个独立 link，端点上下文不相互串用

#### Scenario: Different spellings of one endpoint reject before any write
- **GIVEN** 两个已登记 alias 指向同一 store，引用 r0、r1 均合法且属于同一范围链
- **WHEN** 本次命令分别通过这两个 alias 把 r0、r1 都作为 link-from
- **THEN** 报告同方向同对象重复；不留下 BEGIN、范围成员、link 或 inbound 保护凭据

#### Scenario: Independent same-endpoint links still require individual adaptation
- **GIVEN** 先前命令已创建 A 到 B 的 L1
- **WHEN** 用户在新命令仅指定一次相同方向端点，并通过检查
- **THEN** 创建不同 ID 的 L2，不合并 L1；日后适配 L1 不同时处理 L2

### Requirement: Structured query results do not require parsing display labels

范围、历史、tree、link 与诊断的结构化输出 SHALL 将 store 上下文、对象类型、链首 ID、tip、有效范围版本、来源版本、所属文件、项目相对路径及位置分字段表达；不适用字段使用 null。link ID、对象端点和用于复核的 commit 版本 SHALL 分开。人类可读标签可以包含路径和坐标，但 MUST NOT 是写入或内部身份解析的必要依据。

删除本机查询缓存后，系统 SHALL 从权威记录与显式本机映射恢复相同的对象、关联及版本关系，不从标签或当前路径猜测历史。必要来源版本缺失 SHALL 如实诊断，不以空正文、另一范围或最新内容补齐。诊断与退出码 SHALL 表达实际用法、版本、锁、I/O 或执行错误，不因解析发生在组合操作内就统一报锁冲突。

#### Scenario: A rebuilt index preserves independent ranges
- **GIVEN** 同一文件同一坐标有两个独立范围及各自 link，权威依据完整
- **WHEN** 用户移除查询缓存后查看 tree 和 link
- **THEN** 两个范围的链首 ID、坐标、tip 和 link ID 都与重建前一致，不合并为一个位置键

#### Scenario: Invalid endpoint input is not a lock conflict
- **GIVEN** 存储没有锁冲突，输入端点类型错误
- **WHEN** 组合命令以 JSON 返回失败
- **THEN** 返回用法诊断与相应退出码，不报告 lock_conflict；没有发布则不声称存在部分成功成员

### Requirement: Unsupported formats are refused without migration

新持久格式与结构化输出 SHALL 有明确版本。新程序 SHALL 在使用不支持的记录或 manifest 格式前明确拒绝，不自动迁移、双格式写入、删除或重写已有数据。旧 source-ref 和路径拼坐标端点接口 SHALL 不提供兼容解释；错误 SHALL 指向新的字段输入方式。该接口拒绝 MUST NOT 阻止合法路径本身含有旧分隔符。

#### Scenario: An old-format store is opened by the new program
- **GIVEN** 所选目录含不受支持的旧格式权威数据
- **WHEN** 用户打开它或请求写入
- **THEN** 报告不支持的格式并保持数据不变，不运行其中的来源命令或自动初始化覆盖

#### Scenario: A legacy source option is not treated as a URI fallback
- **WHEN** 用户提供已取消的 --source-ref 选项
- **THEN** 输入被拒绝并提示分字段方式，不尝试旧语法或 URI 解析
