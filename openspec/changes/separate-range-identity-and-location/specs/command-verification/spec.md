## MODIFIED Requirements

### Requirement: A command object uses a fixed executable and literal arguments

command SHALL 作为显式初始化的虚拟文件对象，来源使用分开的类型、executable 与 argv 字段。CLI SHALL 使用 `--source-type command --executable <程序> --args-json <JSON字符串数组>`，不使用 `command::<executable>::<JSON args 数组>` 或 URI 来源语言。executable SHALL 固定，args SHALL 为硬编码 JSON 字符串数组，允许空数组。系统 MUST NOT 增加变量替换、动态参数语言、隐式 shell 或把 argv 拼回 shell 字符串；用户明确选择的 shell 则自行解释收到的参数。

来源描述字段与虚拟文件的登记名称、range 身份、坐标模式 SHALL 分开。参数拆分 MUST NOT 改变显式初始化/replace 的一次采集许可、verify/check 的许可优先级、所属项目根工作目录、失败保留与单次采集复用规则。

#### Scenario: Preserve literal argument boundaries
- **GIVEN** 用户登记的参数数组包含空字符串、空格和字面 `::`
- **WHEN** 该 command 按授权被执行
- **THEN** 参数按原有顺序与边界交给 executable，不因空格或 `::` 被重新拆分

#### Scenario: Reject invalid argument types before launch
- **WHEN** 用户提供的 args 不是有效 JSON 字符串数组，例如包含数值或对象
- **THEN** 系统拒绝该输入，不启动来源程序

#### Scenario: A new descriptor does not grant execution during inspection
- **GIVEN** 已登记 command 使用分开的 executable 与 argv 字段，存在完整成功历史输出
- **WHEN** 用户查看它的范围链、来源版本或重建缓存
- **THEN** 不执行该命令，历史仍从已保存的完整输出恢复
