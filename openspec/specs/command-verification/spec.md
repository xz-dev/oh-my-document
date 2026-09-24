# command-verification Specification

## Purpose

定义 OMD 将外部命令的完整成功输出作为虚拟文件来源时的执行和验证边界。让用户能明确控制命令是否运行，区分旧输出与本次采集，并避免失败输出、隐式 shell 或重复运行破坏确认依据。

> 规划基线：D-28 的初始化采集与 E-6 的执行器契约已确认，D-33 接受工程方案。command 仅在内容取得方式上区别于文件，后续共用跟踪规则。以下要求不批准读取任意配置时执行其中的程序，也不表示已经运行产品测试。

## Requirements

### Requirement: A command object uses a fixed executable and literal arguments

command SHALL 作为显式初始化的虚拟文件对象，来源形式为 `command::<executable>::<JSON args 数组>`。executable SHALL 固定，args SHALL 为硬编码 JSON 字符串数组，允许空数组。系统 MUST NOT 增加变量替换、动态参数语言、隐式 shell 或把 argv 拼回 shell 字符串；用户明确选择的 shell 则自行解释收到的参数。

#### Scenario: Preserve literal argument boundaries
- **GIVEN** 用户登记的参数数组包含空字符串、空格和字面 `::`
- **WHEN** 该 command 按授权被执行
- **THEN** 参数按原有顺序与边界交给 executable，不因空格或 `::` 被重新拆分

#### Scenario: Reject invalid argument types before launch
- **WHEN** 用户提供的 args 不是有效 JSON 字符串数组，例如包含数值或对象
- **THEN** 系统拒绝该输入，不启动来源程序

### Requirement: Explicit initialization captures the first complete command output

用户显式初始化 command 对象时，系统 SHALL 执行所指定的固定命令一次，取得完整 stdout。只有正常 exit 0 且其他写入前置检查通过，才 SHALL 产生初始化 commit 并保存首个完整成功来源版本。显式初始化 SHALL 构成本次采集的许可，不受用于 verify/check 的 `VERIFY_COMMAND_AUTO_RUN=false` 阻止。失败或部分输出 MUST NOT 产生成功的初始化 commit，也不自动重试。初始化成功 MUST NOT 自动确认 range 或声称适配了上游。

#### Scenario: Initialize despite verification auto-run being disabled
- **GIVEN** 用户配置 `VERIFY_COMMAND_AUTO_RUN=false`
- **WHEN** 用户显式初始化 command，命令正常 exit 0，输出完整，写入检查通过
- **THEN** 产生初始化 commit 并保存完整 stdout，后续使用与文件相同的范围跟踪流程
- **AND** 不为初始化后的确认再次执行命令，也不自动确认正文范围

#### Scenario: First capture fails after producing partial output
- **WHEN** 显式初始化的命令输出部分内容后非零退出
- **THEN** 初始化失败，不产生初始化 commit，也不把部分输出作为首个成功来源版本
- **AND** 不把失败冒充合法空输出或宣称撤销了外部程序已发生的副作用

### Requirement: Command execution uses the owning project root

初始化采集、显式 command replace 及以后获准的 verify/check 采集 SHALL 以该 command 所属项目的本地根目录作为工作目录，而非调用 omd 时的目录或外置元数据目录。项目根不可用时 SHALL 报告执行上下文或启动失败，MUST NOT 静默换到其他目录运行。

#### Scenario: Invocation directory and metadata location do not change execution context
- **GIVEN** 项目根为 `/work/app`，元数据在 `/work/records`，用户从 `/work/app/docs` 调用 omd
- **WHEN** 该项目的 command 获准执行
- **THEN** 命令的工作目录是 `/work/app`，相对路径不因调用目录或元数据位置而改变

#### Scenario: An unavailable project root does not trigger a fallback
- **GIVEN** command 所属项目根已不可用
- **WHEN** 用户请求一次来源采集
- **THEN** 报告失败，不改用调用目录或元数据目录执行，不记录新的成功来源版本

### Requirement: Verification and coverage checks share execution precedence

来源命令是否在 verify 或 check 中执行 SHALL 按 `本次所调用命令的参数 > 用户配置 VERIFY_COMMAND_AUTO_RUN > 内置 false` 决定。两个命令 SHALL 共用 `--run-command` 开启本次执行、`--run-command=false` 禁止本次执行；不能将一次调用的参数继承为另一次调用的许可。该用户配置键 MUST NOT 被默认为另一个同名环境变量，也不自动增加项目级执行许可层或 check 专用配置键。

check 获准采集时 SHALL 共用完整成功 stdout、所属项目根、失败保留及单次采集复用规则。未采集或采集失败时，check MUST NOT 以旧输出的覆盖率冒充当前输出的覆盖率，也不能将未取得输出当成合法空内容显示 100%。这不将覆盖门槛并入 verify。

#### Scenario: Default verification and check do not run commands
- **GIVEN** 用户未开启 `VERIFY_COMMAND_AUTO_RUN`，本次没有执行覆盖参数
- **WHEN** 用户运行 verify 或 check
- **THEN** 来源命令不执行，结果报告 command 当前内容未验证
- **AND** 不能把上次输出冒充本次采集成功，check 不能声称当前输出覆盖完整

#### Scenario: CLI disables a configured automatic run
- **GIVEN** 用户配置 `VERIFY_COMMAND_AUTO_RUN=true`
- **WHEN** 用户运行 `verify --run-command=false` 或 `check --run-command=false`
- **THEN** 本次不运行来源命令，并报告当前内容未验证

#### Scenario: CLI explicitly enables this run
- **GIVEN** 用户配置 `VERIFY_COMMAND_AUTO_RUN=false`
- **WHEN** 用户运行 `verify --run-command` 或 `check --run-command`
- **THEN** 本次执行该检查范围内的已登记来源命令并报告采集结果
- **AND** check 对取得的完整成功内容应用共同的范围有效性与覆盖规则，不为统计再次执行来源命令

### Requirement: Only complete stdout from a normal successful exit is content

只有程序正常退出且 exit code 为 0 的完整 stdout SHALL 成为一次成功来源内容。stderr SHALL 独立诊断，不并入跟踪正文。空 stdout SHALL 是合法空内容；stderr 非空而 exit 0 SHALL 不单独构成失败。采集成功 MUST NOT 自动确认 range。

#### Scenario: A successful command prints warnings
- **WHEN** command 正常 exit 0，stdout 为正文且 stderr 非空
- **THEN** 完整 stdout 是本次成功内容，stderr 单独显示
- **AND** 不因 stderr 非空单独拒绝成功内容，也不自动更新范围确认

#### Scenario: A successful command produces no output
- **WHEN** command 正常 exit 0 且 stdout 为空
- **THEN** 采集得到合法空内容，不被当作执行失败

### Requirement: Failed capture cannot advance the source version

启动失败、异常终止、非零退出或未取得完整输出时，系统 SHALL 报告本次采集失败并保留先前成功版本。失败的部分 stdout MUST NOT 成为新的成功内容；历史内容 MUST NOT 被用来冒充本次成功。失败是诊断，不是第四种标记状态。

#### Scenario: Partial stdout followed by nonzero exit
- **GIVEN** command 对象已有成功输出版本 V1
- **WHEN** 本次程序先输出部分内容后非零退出
- **THEN** 报告失败，V1 保留，部分输出不成为新成功版本
- **AND** 不能声称本次当前内容已验证

### Requirement: Captured output is reused for review rather than rerun

一次成功采集 SHALL 形成可引用的完整来源版本。后续确认、clean 或版本校验 SHALL 使用已采集的具体依据，不为确认再执行该程序。输出变化 SHALL 进入范围差异复核，不能沿用过期确认；输出不变 MUST NOT 抵消 unclean 或其他未处理责任。

#### Scenario: Clean does not execute a side-effecting command again
- **GIVEN** 本次已成功采集 command 输出，并有基于该版本的待处理责任
- **WHEN** 用户针对该责任执行 clean
- **THEN** 使用已采集版本，不再次运行 command

#### Scenario: Equal output does not cancel explicit review work
- **GIVEN** range 已因显式 unclean 而脏
- **WHEN** command 重新采集成功且输出字节未变
- **THEN** unclean 的待处理责任仍保留

### Requirement: Loading or rebuilding metadata is not execution consent

系统 MUST NOT 仅因读取配置、查看 log/tree、列举提交或重建缓存而启动其中的来源命令。输出依赖外部程序、环境或网络时，系统 SHALL 不承诺重复执行得到同样结果；确定性只适用于给定已捕获内容、规则及记录的判定。

#### Scenario: Rebuilding a cache from unfamiliar metadata
- **GIVEN** 元数据包含一个用户尚未请求运行的 command
- **WHEN** 用户重建查询缓存或查看历史
- **THEN** 仅使用已有持久记录，不启动该程序

### Requirement: Explicit command replacement authorizes one complete capture

用户显式 replace 到 command 来源 SHALL 授权该次固定命令采集一次，不受 verify/check 的 auto-run=false 阻止，也不改变这些检查的后续执行许可。采集 SHALL 使用所属项目根与共同执行器；完整 stdout 与选定历史来源版本相同且写入检查通过，才更新恢复 binding。输出失败或完整内容不一致 SHALL 保持原绑定、业务记录与历史不变，不自动重试。之后读取历史 SHALL 使用保存的完整输出，不能重跑 command 来恢复过去，也不把历史 replace 改成当前取得内容方式的变更。

#### Scenario: Replacing with a command does not grant future automatic execution
- **GIVEN** auto-run 为 false，用户显式 replace 某历史版本到 command
- **WHEN** 该命令一次采集成功、完整内容一致且写入检查通过
- **THEN** 更新该历史版本的恢复 binding；以后读取它不运行程序
- **AND** 下一次未显式开启的 verify/check 仍不执行 command

#### Scenario: A successful but different output cannot replace history
- **WHEN** replace 的 command 正常退出 0，但完整 stdout 与旧版本不同
- **THEN** 拒绝替换并保留原绑定，不将成功执行等同于成功 replace

### Requirement: The executor preserves complete output and a fixed invocation context

执行器 SHALL 使用直接 argv，stdin 为 EOF，继承调用者环境/PATH，但不持久保存秘密环境值。带目录的相对 executable SHALL 相对所属项目根解析，裸 executable 用 PATH 查找；固定 argv 仍不允许模板或隐式 shell。stdout/stderr SHALL 并行排空，stdout 流式暂存，不静默截断；初版不施加任意默认超时或输出截断阈值。磁盘、读取、启动、取消等失败 SHALL 保留旧成功版本，未完成材料不作为正文基线；只有正常 exit 0 且收集完整才可接纳。外部副作用不能由 OMD 保证撤销。

#### Scenario: A program waits for standard input
- **WHEN** 获准程序尝试从 stdin 读取
- **THEN** 它收到 EOF，不意外读取 OMD 的交互输入或 JSON 请求

#### Scenario: Both output streams exceed a pipe buffer
- **WHEN** 获准程序大量输出 stdout 与 stderr 后正常退出 0
- **THEN** 两路持续排空，正文保持完整，stderr 单独诊断，不因串行读取而死锁或截断

#### Scenario: Storage fails during capture
- **GIVEN** 上次成功输出仍有完整恢复依据
- **WHEN** 本次暂存 stdout 时写入失败
- **THEN** 本次采集失败，旧版本保留，不能把已收到的前缀当成新成功输出

### Requirement: Difftastic filtering is an explicit one-shot external tool permission

`verify --difftastic` 与 `check --difftastic` SHALL 构成对本次调用显式壳执行外部 `difft` 二进制的许可，与既有 `--run-command` 同一许可哲学：一次性、不携带到下一次调用。不带该参数时，系统 MUST NOT 启动 difftastic 或任何外部 diff 工具。读取、查询、log、tree、reindex 及缓存重建 SHALL NOT 触发外部工具执行；该参数 MUST NOT 被自动配置层默认开启。

系统 SHALL 以固定 argv 调用 `difft`（无隐式 shell、无变量模板），比较对象为已记录的完整旧内容与本次观察取得的完整新内容，均不落盘到共享权威存储之外的可推测位置。difftastic 可执行文件缺失、启动失败或自身非零退出时，SHALL 如实报告工具失败，将相关范围归为无法分类，MUST NOT 静默视为结构无变化，也不因工具失败使整个 verify 直接崩溃到无报告状态。

分类证据 SHALL 记录工具身份与版本（如 `difft --version` 输出）、旧/新来源版本依据及判定结果；该证据 SHALL 在后续 `commit cosmetic` 确认时随提交落库。

#### Scenario: No flag means no external tool execution
- **GIVEN** 用户配置中存在可用的 difftastic 二进制
- **WHEN** 用户运行不带 `--difftastic` 的 verify
- **THEN** 系统不启动 difftastic，报告行为与现状完全一致
- **AND** 不存在自动开启该过滤的配置层

#### Scenario: One call's permission never carries into the next
- **GIVEN** 用户刚以 `--difftastic` 完成一次 verify
- **WHEN** 用户随后运行同目录的另一次 verify
- **THEN** 后一次调用不执行外部工具，除非它自己也带 `--difftastic`

#### Scenario: A missing binary fails honestly instead of passing everything as cosmetic
- **GIVEN** `--difftastic` 已指定但 PATH 中没有可用的 `difft`
- **WHEN** 用户运行 verify --difftastic
- **THEN** 相关范围被归入无法分类并保守保留在 dirty 报告中，报告包含明确的工具失败诊断
- **AND** 不把任何范围标为结构无变化，也不让整个命令崩溃到无 JSON 输出
