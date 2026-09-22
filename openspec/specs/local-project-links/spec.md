# local-project-links Specification

## Purpose

定义 OMD 在显式登记的本地元数据上下文中组织项目、解析 alias 和表达跨项目范围关联的行为，包括各自保留独立元数据目录的项目。确保目录当前位置、稳定项目身份及提交内容版本分别处理，用户可以组合多个项目而不依赖自动远端获取或强制身份旁路。

> 规划基线：D-32 的内容/link 覆盖、tag 继承和方向规则，以及 D-33 接受的 E-4/E-5 跨存储与登记契约已确定。以下地址和命令是产品规格，不构成当前会话联网、clone 或写产品数据的授权。

## Requirements

### Requirement: Project aliases bind explicit local directories

alias SHALL 在组织关联的元数据上下文中显式登记，并绑定用户准备的本地目录，而非将整个项目固定在一个内容快照。对其中登记的真实文件，当前观察 SHALL 读取当前路径所指文件，包括未提交修改；历史内容通过 Git 引用取回也不能改变这一点。历史 Git 定位 SHALL 按 managed-content-tracking 契约解释，不改变 alias 的目录绑定性质。`proj:A:file_path` SHALL 通过 alias A 解析；`proj:root:file_path` SHALL 指当前项目。OMD MUST NOT 根据来源标识自动下载、clone 或更新远端，也不根据同名目录自动换绑。

#### Scenario: The linked directory changes after registration
- **GIVEN** alias A 已绑定一个本地项目目录
- **WHEN** 用户修改该目录中的来源文件后执行检查
- **THEN** 检查该本地目录的当前内容及其 OMD 版本依据
- **AND** 不将关联固定在登记目录时的永久内容快照

#### Scenario: A Git-shaped source label is not a fetch command
- **WHEN** 用户登记 `git:github.com/someprojectpath` 作为来源身份信息
- **THEN** OMD 不自动访问该地址或获取代码

### Requirement: Optional remote identity checks do not introduce force

Git 来源身份校验 SHALL 仅在用户显式配置该信息时生效。配置后 SHALL 按登记身份及显式映射核对本地实际 remote；不匹配时 verify SHALL 报错并提示用户更新正确的 alias/身份登记。系统 MUST NOT 提供运行时 force 旁路或静默接受、改写登记。未采用 Git 历史引用、也未配置此身份约束的普通目录跟踪 MUST NOT 依赖 Git。可选 remote 身份核对 SHALL 与 Git 历史内容读取分别判断：后者需要本地仓库对象，不因此强制配置 remote 身份约束或改变 project_id，也不将当前文件观察改为读取 remote 或 HEAD。

#### Scenario: Remote changes without a matching mapping
- **GIVEN** 已配置的来源身份与本地 remote 不再对应，且没有显式认可的映射
- **WHEN** 用户运行 verify
- **THEN** 报告身份不匹配，提示修正登记
- **AND** 不提供 `commit link --force` 绕过该约束

#### Scenario: Non-Git directories remain supported
- **GIVEN** 项目是普通目录，未配置 Git 来源身份
- **WHEN** 用户进行内容跟踪和范围关联
- **THEN** 不要求 Git 仓库、remote、commit、index 或 blob

### Requirement: Project registration identity does not rewrite commits

project_id SHALL 为独立持久登记元数据，首次登记时生成，不由本机目录、当前 remote 或内容 hash 自动推导。复制项目、移动目录或更改 remote SHALL 不自动改变该身份或旧 commit ID；显式身份迁移 SHALL 记录旧新身份对应。project_id MUST NOT 直接或经操作载荷重新进入 commit hash。

#### Scenario: A project is moved locally
- **GIVEN** 项目已有登记身份与保留的 commit
- **WHEN** 用户移动本地目录并显式修正登记路径
- **THEN** 原 project_id 与旧 commit ID 保持不变
- **AND** 不能以本机新绝对路径重算旧提交

### Requirement: Cross-project relationships preserve the range boundary

系统 SHALL 允许已显式关联的本地项目之间建立以 range 跟踪对象为端点、以 link ID 区分实例的关系，并从关系两端查询。范围的后续 commit SHALL 不被误认为新的关联端点；同方向同端点的多个实例仍按 link ID 区分。跨项目关系 MUST NOT 合并各自的前驱历史，也不能以文件级泛链接代替用户指定的范围对应。查询的反方向 SHALL 不被当作新增的业务反向 link。

两个项目各有独立权威元数据目录时，系统 SHALL 支持显式登记本地对方目录后直接引用，MUST NOT 要求先将记录搬入同一目录或隐式复制对方权威记录。依赖双方记录的查询与写入 SHALL 核对参与方的版本及可用性；不可用或与调用方依据不符时 SHALL 报告相应错误，不用缓存或另一同名目录冒充所指权威版本。跨存储引用 SHALL 采用下方保护凭据协议，不把单链 ATOMIC 扩大为跨存储业务事务。

#### Scenario: Relate implementations in two languages
- **GIVEN** 两个本地项目已显式登记，并分别存在 C 和 Python 实现的范围跟踪对象
- **WHEN** 用户选择这两个 range 建立关联
- **THEN** 系统记录带 link ID 的跨项目范围关系，可从两端查到该实例
- **AND** 两个范围各自的提交前驱链保持独立

#### Scenario: Link ranges without merging two metadata directories
- **GIVEN** A 和 B 分别持有独立的权威元数据目录，已显式登记本地引用，所选范围与版本均有效可用
- **WHEN** 用户建立 A 中范围到 B 中范围的 link
- **THEN** 关系可以建立，两边继续使用各自的权威记录，不要求搬迁或隐式复制
- **AND** 对方目录不可用或写入依据发生版本冲突时，不以缓存伪装成功，也不自动换用其他版本

### Requirement: Tags and named checks express relationship requirements

用户 SHALL 能在 import 时或之后通过 `commit tag` 为文件夹或文件设置 tag，并定义 tag 之间的单向或双向 range link 检查规则。规则 SHALL 属于由 `check` 执行的命名检查集合，不是新的标记生命周期，也不是 verify 隐式强制执行的覆盖门槛。系统 MUST NOT 因声明规则而自动补造范围、link 或不存在的实现。

目录 tag SHALL 递归作用于成员，包含之后新增的文件。文件或子目录增加 tag SHALL 与继承的 tag 叠加，而不是替换；同名 tag 的重复继承 MUST NOT 重复计算内容。统计仍 SHALL 遵守有效 import/remove/exclude/include 范围，不因继承 tag 恢复已移出的统计内容。

单向规则 SHALL 只要求指定方向的内容覆盖，不能因额外反向 link 单独判违规。双向规则 SHALL 分别要求两个方向达到完整覆盖，不能把反向查询当成反向 link。

#### Scenario: Declaring a rule does not invent an implementation
- **GIVEN** 用户希望 spec 对应 code，但实际代码或相应 range 尚未存在
- **WHEN** 用户声明 spec 与 code 的 link 规则
- **THEN** 规则可以保存；用户运行 check 时，未满足的内容/link 覆盖作为缺口报告
- **AND** 不要求声明当下自动完成代码，也不创建虚构 link 来通过检查

#### Scenario: A child tag does not replace an inherited tag
- **GIVEN** `docs/` 标为 spec，`docs/examples/` 另标为 example，目录位于有效统计范围内
- **WHEN** 用户检查其中已有或后来新增的文件
- **THEN** 文件同时属于 spec 和 example，分别纳入对应规则
- **AND** 同一 tag 从多个祖先继承时不重复计数

#### Scenario: A one-way requirement permits additional reverse links
- **GIVEN** `spec -> code` 已有完整合格覆盖，其中一些范围还存在 `code -> spec` 的 link
- **WHEN** 用户执行这条单向规则的 check
- **THEN** 不因额外反向 link 使规则失败
- **AND** 若执行双向规则，仍须分别检查两个方向的完整覆盖，不能仅靠正向满足

### Requirement: Tag relationship coverage measures content rather than object counts

check SHALL 按所检查 tag 的文件或文件夹内应计的非空内容统计 link 覆盖，100% SHALL 表示这些内容都至少被符合所选关系的有效范围关联覆盖一次。只有标记而没有对应关系 MUST NOT 冒充该关系的 link 覆盖。分母 MUST NOT 被替换为 range 个数、文件个数或“整个 tag 是否至少有一条边”；应计范围内尚无 range 的内容也 SHALL 留在覆盖分母中。

多个合格范围 SHALL 能共同覆盖内容，重叠位置只计一次；同内容的重复范围或平行 link ID MUST NOT 补足其他位置的缺口，也不合并各自的处理责任。已有“只有未过期已确认范围可贡献有效覆盖”的限制 SHALL 保持。文本空白的统计过滤及空内容显示 SHALL 遵守 managed-content-tracking 的覆盖契约，不删除原文或改变坐标。目录 tag SHALL 按上述继承叠加规则解析。

#### Scenario: One linked fragment does not cover the rest of a file
- **GIVEN** 应计内容为 `ABCD`，只有 `AB` 具有符合所选 tag 关系的有效关联，`CD` 尚未关联
- **WHEN** 用户运行该关系的 check
- **THEN** 不能报告 100% link 覆盖，结果保留 `CD` 的缺口
- **AND** 重复标记 `AB` 或为它增加同端点的 link 不提高这段内容的覆盖量

#### Scenario: Full content coverage does not require every overlapping range to have a link
- **GIVEN** 应计内容 `ABCD` 已由两个合格关联范围 `AB`、`CD` 覆盖，另有一个独立范围 `BC` 没有 link，其他条件均满足
- **WHEN** 用户运行该内容范围的 link 覆盖检查
- **THEN** 该内容范围覆盖率为 100%，不因额外的 `BC` 范围没有 link 就判内容未覆盖
- **AND** 不合并范围或 link 身份，也不因此处理其他独立待办

### Requirement: Rule severity and skipping are explicit

用户请求的 check 规则 SHALL 支持声明 `--level=warn|fail`，默认 fail。warn SHALL 只提醒，不因该规则单独使 check 失败；fail 规则未满足 SHALL 导致该项 check 失败。其覆盖缺口 MUST NOT 被默认为 verify 的强制失败条件。用户显式跳过或关闭检查 SHALL 不消除原关系缺口或授予确认资格。

#### Scenario: Warn reports insufficient coverage without failing the check by itself
- **GIVEN** 一条命名规则的 link 覆盖要求未满足，其他检查均满足
- **WHEN** 用户通过 check 以 warn 级别运行该规则
- **THEN** 报告覆盖缺口与警告，不因该规则单独使 check 失败

#### Scenario: Fail applies to the requested check rather than forcing verification
- **GIVEN** 一条规则的覆盖要求未满足，verify 自己的条件均满足
- **WHEN** 用户通过 check 以 fail 级别运行该规则
- **THEN** 该项 check 报告失败，覆盖缺口保留
- **AND** 不据此改变 verify 的结果，也不自动确认或创建任何范围

### Requirement: Tags have project-local names

tag SHALL 在项目内使用平面名称，同名表示同一个 tag；不同项目的同名 tag SHALL 不自动共享身份。跨项目规则 SHALL 显式指定对方项目的 tag，不通过目录名或字符串相同猜测目标。

#### Scenario: Two projects both use the spec tag
- **GIVEN** 项目 A 与 B 均有名为 spec 的 tag
- **WHEN** 用户在 A 中引用本项目的 spec tag
- **THEN** 不自动将 B 的 spec 当作同一个 tag
- **AND** 跨项目规则必须明确所指项目

### Requirement: Cross-store publication protects referenced versions first

每条业务关系或适配记录 SHALL 仅在所属存储保存一份权威记录；外部保护凭据不是第二份业务关系。写入 SHALL 枚举必要目录并按规范绝对路径顺序取得独占锁，核对观察凭据；锁冲突、旧版本或新增未核对依赖 SHALL 拒绝本次发布，不无界追锁或自动采用新依据。

A 的拟发布记录引用 B:b1 时，B SHALL 先持久保存 inbound 凭据，记录 A 的身份/定位、拟发布记录 ID 和确切保护目标，A 再发布业务记录。两端查询 SHALL 依据 A 实际有效的业务记录，而非仅凭 B 的凭据认定 link 成立。A reset 后凭据不恢复其有效性，也不自动删除凭据。B gc SHALL 核对 A 的有效记录及必要传递保留依据；A 不可读时保守保护相关目标，不能因超时、删 alias 或缓存缺项而释放。

保护闭包 SHALL 包含所需 previous_id、来源版本/完整内容、link/适配依据及块结构依据；不能仅保护直接目标而删除其恢复依赖。该协议不保护外部 Git 对象免受用户 gc，也不授权创建 Git refs。正常本地操作 MUST NOT 因无关 peer 离线而阻塞；实际依赖离线时查询 SHALL 不完整，而非“无 link”或已完整覆盖。

#### Scenario: The consumer fails after the protection receipt is durable
- **GIVEN** B 已保存保护 b1 的凭据，A 尚未发布对应业务记录
- **WHEN** A 的写入失败或进程中断
- **THEN** 没有有效新 link，但 b1 暂受保护
- **AND** 只有显式 gc 核对 A 未发布且无其他保护需要后才能回收孤立凭据

#### Scenario: An offline consumer prevents unsafe collection
- **GIVEN** B 有 A 引用 b1 的保护凭据，A 当前不可读取
- **WHEN** 用户在 B 请求 gc
- **THEN** b1 及所需传递内容被保留并列出原因，其他可证明安全的候选可按规则处理
- **AND** 不用缓存断言 A 已放弃引用

#### Scenario: An unrelated offline store does not block local work
- **GIVEN** 本次本地操作的必要依据均可核验，另一个已登记但无关的 peer 离线
- **WHEN** 用户请求本次操作
- **THEN** 不仅因该无关 peer 离线而拒绝

### Requirement: Store copies and moves preserve explicit authority

存储 SHALL 有独立 128-bit store_id，与 project_id 和业务 commit ID 区分。跨存储端点 SHALL 固化目标 store_id、范围身份与必要版本；本地端点使用 local 作用域。目录移动 SHALL 显式更新定位而保持 store_id。副本作为新的可写权威目录时 SHALL 显式登记新 store_id，保留 project_id、旧 commit 输入和 ID，并补齐仍有效或需保护的外部引用登记；登记未完成时只允许历史诊断读取，不允许新业务写入或 gc。同名目录/alias MUST NOT 静默接管已有引用。

#### Scenario: A writable copy is not silently treated as the original consumer
- **GIVEN** 用户复制 A 的元数据目录，尚未为副本登记新的存储身份与外部引用
- **WHEN** 用户尝试从副本发布业务更新或 gc
- **THEN** 操作被拒绝并要求登记；原提交仍可供历史诊断读取
- **AND** 登记副本不重算 project_id 或已复制 commit ID，不自动让原来的 link 指向另一副本

### Requirement: Local discovery and source parsing are deterministic

配置/缓存路径 SHALL 先尊重 OMD_CONFIG_PATH、OMD_CACHE_PATH，再使用 XDG/平台后备；空值视为未指定，相对路径相对调用 cwd，显式错误不得回退。项目根 SHALL 按显式 --root、本机包含 cwd 的最深已登记根、向上最近可识别 OMD 结构的顺序选择，不用 Git 猜根。元数据 SHALL 按显式路径、本机映射、根下 .omd/、根直属子目录中唯一 manifest 的顺序选择；多个同优先级候选报歧义，不递归全项目扫描。不存在目标只可由显式初始化创建。

来源解析 SHALL 先识别固定前缀，再把 proj 剩余部分保留为完整路径；command 的 argv 部分整体按 JSON 字符串数组解析，不全局按 `::` 切开。不能无歧义表达的字段 SHALL 允许通过 --source-json 对象输入，不能以此引入模板。可选 remote 校验 SHALL 明确指定 remote 名，比较配置 URL 与显式认可映射；不自动猜 SSH/HTTPS 等价或抹除用户名、大小写、端口；缺失/不可读/不匹配 SHALL 报错并允许用户通过登记诊断入口修正。

#### Scenario: An explicit bad metadata location does not select a convenient fallback
- **GIVEN** 用户指定的元数据位置错误，但项目下另有可发现目录
- **WHEN** OMD 解析本次上下文
- **THEN** 报告显式位置错误，不改用另一目录写入

#### Scenario: Two alternative metadata directories are ambiguous
- **GIVEN** 未指定元数据路径，根下无 .omd/，直属两个子目录均有有效 manifest
- **WHEN** OMD 发现元数据
- **THEN** 报歧义并要求明确选择，不按遍历顺序任取一个

#### Scenario: Equivalent-looking remotes still need a declared mapping
- **GIVEN** remote URL 改成另一种 SSH/HTTPS 写法但没有相应认可映射
- **WHEN** 执行已配置的身份检查
- **THEN** 不自行当作等价通过；要求修正登记或提供显式映射
