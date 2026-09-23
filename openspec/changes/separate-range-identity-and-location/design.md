## Context

动机见 [proposal.md](proposal.md)。本 change 在已同步的四份主规格上提出增量；前序 change 保留，不归档。这里的设计不是对当前实现已达标的声明。

已有实现已经有结构化 Range、来源版本 Binding 和独立 link_id；问题不是“所有数据只有一个字符串”。已检查的入口仍由 `src/main.rs` 的 resolve_range_key/parse_span、`src/relations/node.rs` 的 range_key/parent_of 将路径与范围串成节点 key，再反向解析。state 的 tips、mounts、dirty、open_blocks 及关系端点沿用这些 key；rename 会重建子键。相同坐标目前借 nonce 区分，不能误称它们必然冲突。

已观察到 `we@ird.md` 关联失败，以及普通关联示例的 `version record missing`。后者根因尚未证明与分隔符问题相同；分别设置回归验收，不把模型重构当作已修复证据。当前 Cargo package 已有 clap、serde、serde_json、toml 及 Rust/Cucumber 测试设施，优先复用。

契约依据：前序 design D-17、D-24、D-29～D-33、E-2～E-6；[调研笔记](../../../docs/research/traceability-identities.md) 只提供对照，不替代这些决定。早期 `docs/` 与 AGENTS 中的复合来源语法是旧契约，实施本 change 时须同步相关说明；不将早期“尚无实现”的状态描述当作当前事实。

## 自管理使用中的补充决定

用户确认 import 可直接指定文件或目录。统计对象和正文对象沿用各自不可变 Import/Init 链首，在同一路径独立定位；不新增 range 身份、不改既有 ID、不迁移历史。路径查找按用途选择，旧的纯 State 查询遇到多对象不得任取。通用 tag/rule 命令遇到同路径歧义要求显式 tip；import/remove 与正文操作分别选择对应对象。

当前有效统计条件取 tip 向前最近的 Import/Remove，不被后续 tag/rule 遮蔽。文件只统计自身，目录递归；缺失或遍历错误使 check incomplete/失败。没有新序列化字段，但旧程序会拒绝同路径双对象，使用方必须升级二进制，不把同版本号视为可互换。既有旧 init→import 记录原样保留；新统计操作另建独立链。

这是原 24 项审查之后的补充，证据见根目录 traceability；不改变 5.1 未完成状态。

## Goals / Non-Goals

**Goals:**
- 用最少的具名字段区分“对象是谁”“版本是什么”“位于哪里”“在哪里取得完整内容”。人和 Agent 使用同一输入及校验。
- 所有消费者共享一次 commit→对象解析，不在 CLI、关系、覆盖或 reset 中分别猜 key。
- 项目内逻辑位置可共享，本机目录和缓存实例可变；旧 commit 可复算，必要引用不可静默换绑。

**Non-Goals:**
- 不新增 URI、range UUID、来源表达式语言、双格式兼容、迁移器或通用定位框架。
- 不自动联网、同步副本、合并冲突、创建 Git refs、增加分布式锁或承诺语义等价。
- 不改三态、范围关联方向、逐跳内容责任传播、覆盖统计单位、ATOMIC 逐次发布及 reset 边界规则。
- 不顺带清理所有错误类型、拆分整份 main.rs 或解决既有的全部平台验证缺口。

## Decisions

### 1. 范围对象等于一条提交链，不等于位置

继续以同对象 previous_id 链的首个 commit ID 作为对象身份。使用现有 16 字符随机盐产生独立首提交，不增加位置 nonce 或 range UUID；同位置的两次独立创建得到不同链首。

逻辑模型如下；名称是本 change 的实现方案，不是已经发布的 API：

| 字段/概念 | 含义 | 不可替代为 |
| --- | --- | --- |
| `ObjectRef { scope, kind, root_commit_id }` | 所选本地 store 或确切外部 store 下的文件/range 等对象 | 路径、范围、project_id |
| `CommitRef { scope, commit_id }` | 一条确切记录及引用上下文 | 对象的当前 tip |
| `tip_id` | 当前有效前驱链末端，可能是标记或业务元数据操作 | 最新有正文记录 |
| `effective_range_commit_id` | 当前坐标及完整来源依据所属的有效版本 | BEGIN/END 的空正文 |
| `source_version_id` | 完整内容、视图及采集证据；沿用既有独立版本 ID | range ID 或仅片段 hash |
| `Position { mode, start, end }` | 某次范围版本内的位置 | 永久对象身份 |
| `link_id` | 一次显式关系实例的独立 ID | 端点对或创建 commit ID |

本地 scope 不嵌入自身 store_id；外部 scope 明确目标 store_id。文件/range/import 的身份遵守同一链首规则，虚拟 root 保持特殊对象且不可 reset。对象引用到父文件与同对象 previous_id 分开：范围不沿父文件链找自己的 genesis。

首提交没有 root_commit_id 自引用；先规范化 payload 并求 commit hash，求得的 ID 就是 root。后续记录由 previous_id 证明所属链。允许在 state/索引中投影 root，但必须能从权威记录重建并校验。文件记录的范围恢复表使用 `root_commit_id -> tip_id`，不按位置或时间戳恢复。

**取舍：** 不补 `rfind('@')`，不更换分隔符/加转义，也不加第二套随机范围身份。只换分隔符不能解除 rename、同坐标对象和下游索引对位置的依赖。

### 2. 解析 commit、校验可用性、选择版本是三件事

以下实现边界是既有契约在修复中明确的约束，不增加产品要求；对应回归与证据见 [任务证据表](../../../spec-traceability.md)。

统一解析顺序：选 store/alias → 解析完整 ID 或该 store 内唯一前缀 → 校验记录类型/原始输入 → 沿同对象前驱解析 root → 检查本次用途所需的有效性与观察凭据。

- log 可以查看保留的 dangling 记录；解析出 root 不授予写入或复核资格。
- 普通 `--id` 必须是当前有效 range tip；历史 ID 不会偷偷升级为 tip。
- link 输入 commit 用于找持久端点，并保留明确选择的版本依据。链推进不重建关系；引用不可达时仍报 broken，不换成最新版本消除责任。
- 同命令重复检查使用 `(方向, 解析后的实际store_id, root_commit_id)`；local/external 或两个 alias 指向同一 store 时先规范化。不存在或类型错误的端点在 BEGIN、inbound 或业务发布前拒绝。
- 物理发布/登记/本机实例凭据与业务前驱分别校验，保持 `--expected` 拒绝旧依据及不重试。
- 写入在锁内校验调用者提交的必需依据及实际映射修订，不以入口现取的快照替代。成功观察的版本记录、声明 hash 与完整实际字节必须一致；不一致时拒绝，不另造版本补救。新鲜采集到的变化内容可作为新正文依据，与复用过期凭据的负例分开；失败或未许可采集不能回填历史成功证据。

**取舍：** 不靠 path+range 选择“唯一看起来匹配的对象”，不把 --id 复用为 link ID，不允许尚未存在的前向端点。

### 3. 有效范围状态与空正文标记分开

单链状态折叠以操作类型为准。范围版本提供 Position、视图/编码及 source_version_id；BEGIN/END 只改变块结构；link/clean/unclean 的业务作用保留，不能因正文空就跳过。标记的继承版本依据必须可从其 payload/同链有效前驱解释，不跨到文件或另一个范围补内容。

新范围组合 `BEGIN s -> range r -> link ... -> END e` 中：root=s，tip=e，有效范围版本=r。只有 BEGIN 时有效范围版本为空、覆盖贡献为零，闭合检查失败但不是第四种标记状态。

reset 改变有效前缀而非 root：普通块外目标保留本身；BEGIN/END 撤下自身并且只退一次直接前驱；内部普通成员仍拒绝。首 BEGIN 退空链并撤下挂载，不补前驱。之后无 --id 的创建是新链，不按坐标复活 s。文件 reset 使用所存范围 tip 精确恢复，记录的 END 不被再退一次。GC 保护闭包包含保留记录所需的链首、前驱、有效版本和结构标记；不因 root 不再是 tip 就删除它。

**验收边界：** 普通关联示例必须真实运行 verify；“END 可以解释到 r”是模型要求，不证明现有 `version record missing` 已有唯一根因或已修复。真正丢失 V 时仍须失败。

### 4. 来源默认 file；路径、类型、模式和 Git 字段拆开

这里只保留一套独立 CLI 字段，转换为封闭的类型化来源描述。具体参数拼写及分组是本 change 提出的单一工程方案，供整体审阅；字段分离、取消 URI、复用 alias 等产品原则已经确认，不表示用户曾逐项确认下面的参数名称。不要引入可扩展 URI parser 或把多个字段重新拼成字符串。

| 用途 | 参数方案 | 默认/校验 |
| --- | --- | --- |
| 操作对象的位置 | `<path>`，`--project <alias>` | alias 默认 root；command 的 path 是虚拟文件登记名称 |
| 范围 | `--range <start> <end>`，`--mode text|byte` | 新范围 text；续改省略 mode 则沿用；0 起点左闭右开 |
| 来源种类 | `--source-type file|command|git` | 新来源默认 file，不重新解释已登记对象 |
| 文件恢复来源 | `--source-project <alias>`，`--source-path <path>` | 来源项目默认 root；普通 file init 省略来源路径时用目标文件位置；replace 到 file 必须明确来源路径 |
| Git 历史 | `--source-project <alias>`，`--git-commit <完整ID>`，`--git-path <path>` | 本机映射定位已有仓库；不接受浮动 ref；不能以工作区补缺对象 |
| command | `--executable <程序>`，`--args-json '<字符串数组>'` | 两项必填，允许 `[]`；cwd 是对象所属项目根；不接受另一个 command cwd |
| Git 身份登记 | `--git-remote <name>`，`--git-remote-url <url>` | 在显式项目/alias 登记入口成对设置；不默认猜 origin，不经普通内容命令暗改登记 |
| 文本编码 | `--encoding <encoding>` | 沿用既有显式→范围→文件→项目→用户→UTF-8 优先级；历史保存所用值 |

源端项目与被跟踪对象所属项目可以不同，例如历史内容在另一已登记仓库。来源描述通过既有项目登记引用定位，项目登记身份保存在适当的归属/来源版本元数据中；不将 project_id、本机绝对根或整个 manifest 塞回业务 hash。来源版本先于业务 commit 创建，commit 的 content_ref 固化该版本；重定位不改写原始 descriptor 或已提交 payload。

来源静态字段和适用的文本编码标签在执行 command 前校验；路径规范化、观察完整性和发布资格由 CLI 与库入口共享边界落实，不能只在参数解析层设防。byte 模式不进行文本解码。

路径参数表示选定项目内位置。CLI 与库发布入口均须把接受的、明确属于所选根的绝对输入转换为项目相对路径；词法逃出所选根的输入拒绝并提示选择/登记相应 alias，不偷偷创建新项目。磁盘符号链接仍按既有 follow 规则处理，不把该词法约束偷换成禁止 follow。共享路径使用 `/`，保留文件名字面字符，不做 URL decode。以 `-` 开头的路径使用 CLI 的 `--` 参数边界。

Git 恢复描述保存确切 commit 和该提交内 path，绑定修订可改变恢复方式但不能改完整内容。当前真实文件始终按登记目标路径读现状；Git 历史位置与当前路径可以不同。file/command 保存完整原字节，Git 复用本地对象；不执行 textconv/filter/lazy fetch。command 的授权、一次完整 stdout、exit 0、stderr 分离及失败保留完全沿用基线。

**取舍：** remote URL 仍可以是 Git 自己支持的 URL 值，但不是 OMD source URI。用户已撤回 URI 方向，理由是 scheme/组合/转义及解析成本不断扩张，对人和 Agent 都不友好；不保留旧 source-ref/source-json 作为备用来源语言。

#### 首次初始化前的编码配置

真正的新项目允许在首次显式 init 前，仅预置默认元数据目录中的 `.omd/omd.toml` 编码配置。该目录不得含既有身份、记录或其他存储内容，也不得已有绑定该目标的本机映射。配置文件本身不构成已初始化权威，不授予普通写入资格；读取配置不触发来源执行或自动初始化。

首次 init 沿用未登记目标前置条件和既定配置校验、编码优先级、text/byte 与 command 许可规则，保留配置原字节。已有、损坏或已绑定目标仍拒绝覆盖初始化，不自动修复、迁移或换绑。

### 5. 可重复端点在每次选项内绑定上下文

本地：`--link-from <commit>` / `--link-to <commit>`。
跨 store：`--link-from-store <alias> <commit>` / `--link-to-store <alias> <commit>`。

每个跨 store 选项固定两个值，clap 解析为一条端点结构；不是两个靠下标配对的列表，也不设置影响后续端点的全局“当前 peer”。保留 argv 出现顺序用于组合成员顺序，两个方向和本地/外部形式均可重复。显式 `commit link` 的已有 source/target 选择入口同样接受分开的 store 与 commit 字段，不另用路径拼坐标文法。

以下是计划中的输入示意，不是当前可运行命令；ID 与 expected 值须来自实际结果：

```text
omd commit commit 'we@ird.md' --range 0 5 --mode text --reason 'track text' --expected <observation>
omd commit commit retry.py --range 0 15 --link-from <spec-range-commit> --reason 'implementation' --expected <observation>
omd commit commit retry.py --id <current-tip> --range 0 15 --link-from-store peer <peer-range-commit> --link-to <local-range-commit> --reason 'relate ranges' --expected <observation>
omd replace <recorded-commit> --source-type git --source-project upstream --git-commit <exact-git-id> --git-path docs/spec.md --expected <observation>
```

每项解析和重复检测在组合任何写入前完成。之后仍是逐次发布的 ATOMIC，I/O 等后续失败报告真实部分结果，不承诺整块回滚。成功项按权威发布状态核对具体 commit、link 和保护凭据；失败 creator 的文件可能已存在，但未被权威状态选入不能算发布成功，也不据此新增清理要求。适配仍为 --adapt 的 link_id/changes/reason，源端阻断仍为 --stop；跨命令同端点新关系仍生成新 link_id。

### 6. 共享逻辑登记与本机实例映射分层，但不增设身份服务

复用既有 registrations、manifest、用户配置及发现顺序；不新增项目命名体系。

| 所在层 | 保存内容 |
| --- | --- |
| 权威共享记录/登记 | project_id、store_id、alias 的逻辑绑定、项目相对路径、对象/commit 引用、显式 remote 身份约束 |
| 本机配置 | 已有登记到实际项目根及元数据目录的映射、所选 checkout 实例、认可的本机 remote 映射 |
| 可重建缓存 | 按本机实例及确切 store 发布状态隔离的派生索引 |

两位同事可将同一逻辑项目分别映射为 `/home/alice/app` 和 `/work/bob/app`；共享位置仍是 `docs/spec.md`。本机映射必须核对目标 manifest 的 project/store 身份，缺失或歧义时报错，不搜索同名目录替代。映射修订纳入物理观察凭据，不进入 commit hash。

一个 project_id 可以对应多个本机 checkout，不能仅用 project_id 命中缓存或决定写入目录。使用既有显式 --root/元数据选择和最深已登记根发现规则选择实例；缓存键至少区分实际元数据实例与相关发布/登记修订。无需为此新增业务 instance UUID。

**保留 E-4 副本规则：** 移动同一个权威 store 可保持 store_id；复制成另一个可写权威目录必须显式登记新 store_id 并补齐外来保护。旧 commit/project_id/link_id 不重算，但完整外部引用的 store 命名空间不会自动变成副本。不同环境读取相同逻辑记录不等于批准两个离线可写副本冒用同一权威身份。本设计不负责在两台机器之间运送或合并记录。

跨 store inbound 凭据保存消费方身份及逻辑定位；个人绝对目录从本机映射解析，不成为共享唯一定位。沿用先保护再发布、离线保守保留和必要目录有序加锁协议；别名修正不能解除保护或重定向已有端点。可选 remote 核对属于本次必须交付的显式映射路径，不能继续只记为旧 change 的 deferred。

#### 同事复制后续改的具体路径

设 Alice 的项目 P 位于 `/home/alice/app`，权威 store 为 S；其中范围链首 r0、当前 tip r2，本地 link L 引用 peer B。Bob 把代码和元数据复制到 `/work/bob/app`。这是前序副本契约的应用，不是新同步协议：

| 步骤 | 允许的结果与边界 |
| --- | --- |
| Bob 配置项目根及元数据目录映射 | 可定位复制来的 P、r0、r2、L，历史原输入/ID 不重算；映射本身不授予副本写入资格 |
| 登记前查询历史或尝试续改 | 历史诊断可读；业务写入和 gc 拒绝，不因路径正确就认为副本是 S 的新权威位置 |
| Bob 显式激活独立可写副本 T | 保留 P 和复制来的 commit/link ID，分配新 store_id T，核对并补齐指向 B 的必要保护登记；必要 peer 不可用时不能假称登记完成 |
| Bob 在 T 取得新观察凭据并续改 r2 | 在 T 的同一范围链追加新 commit，root 仍为 r0，前驱为 r2；已有本地关系延续，不重写 Alice 的 S，也不自动传回新提交 |
| 访问跨 store 关联 | T 所保留的出向引用仍指向明确的 B；别处原本指向 S:r0 的引用仍指向 S，不改成 T:r0。需要关联 T 时由用户显式创建相应关系；S 不可用则如实报告 |

副本激活须按保留 link 的 source/source_version 和 target/target_version 收集必要外部端点，不能只保护出向关联。保护记录分别保存确切创建 commit ID 与独立 link_id；普通关联先构造待发布 commit，持久化其精确保护后才发布同一记录，副本保护则沿用保留记录的创建 ID。取得保护能力不授予普通业务写权限。GC 按确切消费者记录及其必要保留闭包判断保护，消费者离线或映射不可核实时不能当作孤儿释放。

因此，`S:r0` 与 `T:r0` 的链首字节相同但权威命名空间不同。仅移动同一个权威 store 则不走“激活新副本”，显式更新定位并保留 S。这里解决路径可移植及显式副本续改，不承诺两份可写目录共享一条自动同步的历史。

### 7. 原有存储布局容纳新字段；所有读取方一起切换

保留 TOML 权威记录、完整内容目录、版本与可变恢复 binding、发布 state；不拆新 crate/服务或增加依赖。复用现有 Range，给状态索引和关系端点换成对象引用。TOML 中需要复合键的映射可用具名字段的条目数组；仅以原始 ID 为键的表仍可保留。任何显示标签都不是可回读业务 key。

本次持久格式和 JSON envelope 明确提升 schema 版本。所有读取方检查版本；不支持的格式拒绝且不改写。hash 保持 E-2 的分帧/JCS/原字节契约，schema 与不可变新字段纳入载荷；自身份、当前登记/本机路径不进入 hash，外部目标 store_id 仍是必要引用命名空间。

实施联动点：
- `src/relations/node.rs`：停止按字符串前缀/@ 推断父级、类型和范围；改为显式所属引用。
- `src/records/`：commit 解析、state 的 tips/mounts/dirty/open_blocks、文件恢复表、link/适配/inbound、GC、版本 binding 共同使用新字段。
- 来源与发现：将 file/command/git 参数转为同一结构输入，逻辑项目经本机映射取得来源；不得新增命令执行时机。
- `src/main.rs`、`src/output.rs`：CLI 校验、结构化 JSON、tree/log/check 输出；解析错不包装为 lock_conflict。
- 现有 Rust/CLI/BDD 测试及双语 README：从真正 CLI 返回的 commit/link ID 取后续输入，不手造路径拼坐标对象 key。

覆盖统计按来源和坐标单位分别计算并集与分母，再做同单位汇总，不平均百分比或混加 text/byte。文本视图失败不能遮蔽仍可计算的 byte 总量；必要分母未知时，相应汇总为 incomplete、percentage=null，已知小计明确标为 partial，不能丢掉未知来源后报告 100%。覆盖完整也不能清除 dirty、link 或开放块责任。

实现可以保留简单的整 state 重写和有限记录遍历；本 change 不顺带建立数据库主数据模型或后台索引服务。

## Risks / Trade-offs

- **格式破坏性变更** → 无既有用户数据，直接拒绝旧格式；绝不把“不迁移”解释为可以删旧目录。
- **链首查找或有效状态折叠错误** → 新建 BEGIN、嵌套 END、首标记退空、文件恢复表与 GC 保护成组验收；缓存只能加速。
- **同事目录不同与副本权威混淆** → 显式区分 project_id、store_id、commit ID、本机实例；旧外部引用不随 alias 自动接管。
- **部分消费者仍解析旧 key** → 覆盖 rename、tag/import、覆盖统计、历史、reset、GC、peer、JSON；检索残留解析仅作补充，不替代行为测试。
- **已观察缺陷不能由设计推断修复** → `@` 路径、错误分类、普通关联示例各留独立失败重现；缺真实版本的负例必须继续失败。
- **remote 身份子系统原先延期** → 本次安排最小可用登记、比较、显式映射及错误诊断；不扩大为联网验证或 URL 标准化。
- **跨平台持久化/命令 I/O 故障注入旧缺口** → 本次变更涉及的回归照常验证，原未验证平台/场景继续如实列出；不能用历史 46/46 或场景总数宣称全部通过。

## Migration Plan

不做数据迁移或双格式兼容。发布新格式时，新建存储使用新 schema；遇到旧格式只报不支持，保留原数据。用户需要保留旧实验数据时应另行选择相匹配的旧程序，不提供自动降级写入或覆盖初始化。

后续 apply 按“模型与解析 → 生命周期 → 来源/本机映射 → CLI/输出 → 端到端验证与说明”推进。新实现未通过验收前，README 不宣称缺陷已经消失；不把删除旧 fixture 当作正确性证据。

本轮只同步前序四份主规格并写规划文档。新 change 的 delta 不提前同步到主规格，实施任务保持未完成；无产品代码变更、外部来源执行、commit、push 或 archive。
