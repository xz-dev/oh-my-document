## MODIFIED Requirements

### Requirement: Project aliases bind explicit local directories

alias SHALL 在组织关联的元数据上下文中显式登记，指向稳定的项目/store 登记；实际本地项目根及元数据目录 SHALL 由本机显式映射提供，不将整个项目固定在一个内容快照。项目和路径 SHALL 分别提供，CLI 使用 `--project <alias>` 与独立路径；root 表示当前项目，不再使用 `proj:A:file_path` 或 `proj:root:file_path` 的复合来源字符串。

对其中登记的真实文件，当前观察 SHALL 读取本机映射下当前项目相对路径所指文件，包括未提交修改；历史内容通过 Git 引用取回也不能改变这一点。历史 Git 定位 SHALL 按 managed-content-tracking 契约解释，不改变 alias 解析到显式本地目录的性质。OMD MUST NOT 根据来源标识自动下载、clone 或更新远端，也不根据同名目录自动换绑。修改 alias、定位或身份约束 SHALL 是显式登记操作，不得通过来源输入悄悄覆盖已有登记。

#### Scenario: The linked directory changes after registration
- **GIVEN** alias A 已绑定一个本地项目目录
- **WHEN** 用户修改该目录中的来源文件后执行检查
- **THEN** 检查该本地目录的当前内容及其 OMD 版本依据
- **AND** 不将关联固定在登记目录时的永久内容快照

#### Scenario: A Git-shaped source label is not a fetch command
- **WHEN** 用户将 Git remote URL 作为独立的来源身份约束值登记
- **THEN** OMD 不自动访问该地址或获取代码，也不把 URL 解析为携带路径和范围的 OMD 引用

### Requirement: Optional remote identity checks do not introduce force

Git 来源身份校验 SHALL 仅在用户显式配置该信息时生效，remote 名与 URL SHALL 是独立登记字段，CLI 使用 `--git-remote <name>` 和 `--git-remote-url <url>` 成对指定。配置后 SHALL 按登记身份及显式映射核对本地实际 remote；不匹配时 verify SHALL 报错并提示用户更新正确的 alias/身份登记。系统 MUST NOT 提供运行时 force 旁路或静默接受、改写登记。未采用 Git 历史引用、也未配置此身份约束的普通目录跟踪 MUST NOT 依赖 Git。可选 remote 身份核对 SHALL 与 Git 历史内容读取分别判断：后者需要本地仓库对象，不因此强制配置 remote 身份约束或改变 project_id，也不将当前文件观察改为读取 remote 或 HEAD。

#### Scenario: Remote changes without a matching mapping
- **GIVEN** 已配置的来源身份与本地 remote 不再对应，且没有显式认可的映射
- **WHEN** 用户运行 verify
- **THEN** 报告身份不匹配，提示修正登记
- **AND** 不提供 `commit link --force` 绕过该约束

#### Scenario: Non-Git directories remain supported
- **GIVEN** 项目是普通目录，未配置 Git 来源身份
- **WHEN** 用户进行内容跟踪和范围关联
- **THEN** 不要求 Git 仓库、remote、commit、index 或 blob

### Requirement: Local discovery and source parsing are deterministic

配置/缓存路径 SHALL 先尊重 OMD_CONFIG_PATH、OMD_CACHE_PATH，再使用 XDG/平台后备；空值视为未指定，相对路径相对调用 cwd，显式错误不得回退。项目根 SHALL 按显式 --root、本机包含 cwd 的最深已登记根、向上最近可识别 OMD 结构的顺序选择，不用 Git 猜根。元数据 SHALL 按显式路径、本机映射、根下 .omd/、根直属子目录中唯一 manifest 的顺序选择；多个同优先级候选报歧义，不递归全项目扫描。不存在目标只可由显式初始化创建。

首次显式 init SHALL 允许真正新项目预先存在仅含编码配置 `omd.toml` 的默认 `.omd/` 目录；目标不得含既有身份/记录/其他存储内容，且不得有绑定该目标的本机映射。配置 SHALL 按既有规则处理并保持原字节。此例外 MUST NOT 用于覆盖初始化已有、损坏或已绑定权威，也不因读取配置而执行来源或授予普通写入资格。

来源解析 SHALL 根据独立类型字段验证相应具名字段；路径字段保持完整字面值，command 的 argv 字段整体按 JSON 字符串数组解析，不识别 OMD URI 或字符串前缀、不全局按 `::` 拆分。独立 CLI 字段已覆盖所有来源，不保留 --source-json 作为第二套来源接口，不引入模板。可选 remote 校验 SHALL 明确指定 remote 名，比较配置 URL 与显式认可映射；不自动猜 SSH/HTTPS 等价或抹除用户名、大小写、端口；缺失/不可读/不匹配 SHALL 报错并允许用户通过登记诊断入口修正。

#### Scenario: Encoding configuration can precede first initialization
- **GIVEN** 真正的新项目仅预置 `.omd/omd.toml` 中的有效非 UTF-8 默认编码，没有既有身份、记录、其他存储内容或绑定该目标的本机映射
- **WHEN** 用户显式首次 init 对应编码的文本
- **THEN** 按既定规则使用预置编码初始化，配置原字节保持不变

#### Scenario: Configuration-only bootstrap cannot replace existing authority
- **GIVEN** 目标含既有或残缺权威，或已有绑定该目标的本机映射
- **WHEN** 用户尝试将其作为配置先行的新项目覆盖初始化
- **THEN** 拒绝请求，目标权威和本机登记保持不变
- **AND** 不执行来源、不自动修复、迁移或换绑

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

## ADDED Requirements

### Requirement: Shared positions resolve through machine-local mappings

共享记录 SHALL 保存稳定的逻辑登记、对象引用及项目内相对路径，本机项目根、元数据目录及缓存位置 SHALL 属于本机配置。两个工作环境使用不同本地根访问同一已登记项目时 SHALL 保持 project_id、保留 commit ID、范围链身份、link ID 及历史坐标不变；不能要求同事复制另一人的绝对目录。缺少、歧义或身份不符的本机映射 SHALL 报告为定位问题，不自动生成新业务身份、换绑 peer 或当作来源内容变化。

登记映射 SHALL 明确关联 project_id、store_id 与所选本机实例；多个 checkout/worktree 共享 project_id 时，解析 SHALL 使用本次明确实例或既有根发现规则，不因同 project_id 而复用另一实例的内容、观察凭据或缓存。缓存删除 SHALL 不丢失身份或必要版本依据。已知映射更新 SHALL 使旧观察凭据失效，不能把未变的业务 ID 当作跳过实例/登记校验的依据。

不同目录映射 MUST NOT 被当作授权多个独立可写副本共享 store_id。Store copies and moves preserve explicit authority 的副本登记规则 SHALL 保留：新可写权威副本显式取得新 store_id、补齐保护登记，复制来的 project_id 和 commit ID 不重算；旧跨 store 引用不自动重定向到副本。本条提供定位可移植性，不提供跨机器同步、自动合并或分布式锁。

#### Scenario: Coworkers use different checkout roots
- **GIVEN** 相同逻辑项目的相同已保留记录在两个环境中可用，项目根分别映射到 `/home/alice/app` 与 `/work/bob/app`，记录中的路径为 `docs/spec.md`
- **WHEN** 两位用户分别查看该范围并检查当前文件
- **THEN** 各自读取对应本地根的 `docs/spec.md`，原 commit、范围链身份与 link ID 不重算
- **AND** 对不同当前内容分别报告实际检查结果，不保证两个环境有相同结论

#### Scenario: Two worktrees do not share a cached observation
- **GIVEN** 同一 project_id 的两个本机实例具有不同当前文件内容
- **WHEN** 用户明确选定第二个实例并执行检查
- **THEN** 只使用第二个实例的文件与对应权威发布状态，不拿第一个实例缓存声称当前通过

#### Scenario: A registered coworker copy continues its own range chain
- **GIVEN** Bob 的独立可写副本 T 已完成登记和必要 peer 保护登记，保留项目 P、范围链首 r0、当前 tip r2 与 link L；Alice 的权威存储仍为 S
- **WHEN** Bob 取得 T 的当前观察凭据并以 --id r2 提交合法范围修改
- **THEN** T 在同一链首 r0 下追加新 commit，L 的既有本地端点延续，S 的记录保持不变
- **AND** 别处原本指向 S 中 r0 的引用仍指向 S，不自动指向 T，也不自动同步两边的新历史

#### Scenario: A peer mapping cannot silently select a writable copy
- **GIVEN** 既有 link 固化 peer store S，用户另有已登记为 T 的可写副本，二者保留相同的旧 commit ID
- **WHEN** S 的本机映射缺失或被误指到 T
- **THEN** 系统报告定位或身份不符，不因 commit ID 相同而把 link 端点改为 T

#### Scenario: A relocation invalidates an old physical observation
- **GIVEN** 调用方持有移动目录前的观察凭据，用户随后显式更新了本机映射
- **WHEN** 调用方使用旧凭据请求写入
- **THEN** 拒绝过期依据，不更改历史 ID，也不自动换用新映射凭据重试
