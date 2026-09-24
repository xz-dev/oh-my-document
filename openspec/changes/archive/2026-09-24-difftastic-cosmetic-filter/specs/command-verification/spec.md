## ADDED Requirements

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
