# 来源、内容与坐标

本文件记录 `separate-range-identity-and-location` 后的当前接口。`proj:...`、`command::...`、`git::...`、`--source-ref`、`--source-json` 和 `path@span` 是已被取代或明确拒绝的历史形式，不是使用说明。

## 1. 对象、位置和版本分开

| 概念 | 当前表达 |
| --- | --- |
| 对象身份 | 所选 store 中同对象链的首个 commit ID |
| 当前状态 | 当前 tip commit ID |
| 有效范围正文 | effective range commit ID |
| 完整来源依据 | source version ID |
| 位置 | 所属文件、项目相对路径、`mode`、`start`、`end` |
| 关系实例 | 独立 link ID；端点对象与选定版本另存 |

范围创建使用：

```text
omd commit commit <path> --range <start> <end> --mode text|byte ...
```

新范围未指定 `--id` 时创建独立对象；续改必须以当前有效 range tip 作为 `--id`。相同坐标和重叠坐标可以对应多个独立链。路径、坐标、当前 tip 和新增 UUID 都不替代链根身份。

本地 link 端点使用 commit ID；跨 store 端点每项同时给出 alias 与 commit ID：

```text
--link-from <commit>
--link-to <commit>
--link-from-store <alias> <commit>
--link-to-store <alias> <commit>
```

完整 ID 或所选 store 内唯一前缀先解析为确切记录，再确定对象链根。错误 store、歧义前缀、缺失前驱、错误对象类型、旧 tip 或必要 dangling 依据都不得按路径、坐标或最新 tip 补齐。

## 2. 文本与字节坐标

两种模式均为 0 起点、左闭右开 `[start, end)`，允许空范围，并校验 `0 <= start <= end <= 来源长度`。

- `text`：按选定编码解码后的 Unicode scalar value 计数，不按 UTF-8 字节、字形簇或显示宽度计数。
- `byte`：按原始字节偏移计数，不解码；文本空白过滤规则不能套用到 byte 覆盖统计。
- BOM、CRLF、tab 和全部原始字节保留。BOM 是文本视图中的实际第 0 个字符。
- 无效文本解码明确失败，不用替换字符静默改变坐标。

编码优先级：本次 `--encoding` → 已记录来源版本 → 精确文件配置 → 项目默认 → 用户默认 → UTF-8。每次实际使用的编码保存在来源版本中，后续配置变化不重新解释历史。

## 3. 封闭来源字段

来源类型使用 `--source-type file|command|git`；新来源省略时默认 `file`。`--mode` 只选择坐标单位，不选择来源类型。对已有对象省略来源字段时，继续使用已登记观察定义，不把 command/git 重新解释为 file。

### file

- 当前项目目标路径由独立 `<path>` 提供。
- 恢复内容位于其他项目时，使用 `--source-project <alias>` 与 `--source-path <path>`。
- `source-project` 默认 `root`；普通 file init 省略 `source-path` 时使用目标路径。
- 路径保持字面值，不做 URI decode，不按 `@`、`#`、`:`、`%` 或 `::` 切割。
- 明确属于项目根的绝对输入可转换为项目相对位置；词法逃出根的输入拒绝。

### command

```text
--source-type command --executable <program> --args-json '<JSON 字符串数组>'
```

- executable 固定；argv 是硬编码 JSON 字符串数组，可为空。
- 参数顺序、空字符串、空格和字面 `::` 保持；不 eval、不拼回隐式 shell。用户显式选择 shell 时，由该程序解释参数。
- cwd 是被跟踪对象所属项目根；stdin 关闭。
- 只跟踪完整 stdout。只有正常退出且 exit code 0 才成功；stderr 独立诊断。
- exit 0 + 空 stdout 合法；非零退出后的部分 stdout 不成为新基线。
- 显式 init/replace 每次只执行一次并复用该观察。普通查询、历史读取和缓存重建不执行命令；verify/check 仍受显式许可控制。
- 采集成功不自动确认范围，执行失败也不产生第四种标记状态。

### git

```text
--source-type git --source-project <alias> \
  --git-commit <完整确切对象 ID> --git-path <该提交内路径>
```

- `source-project` 默认 `root`；项目通过本机显式映射定位。
- 只读取本地已有 Git 对象，不接受浮动 ref，不 clone/fetch，不切换工作区，不创建 Git commit/ref。
- 读取完整原始 blob；不执行 textconv、filter 或 lazy fetch。提交内符号链接按同一提交解析，并检测断链/循环。
- 历史路径与当前登记路径可以不同。历史内容来自指定 commit/path；当前观察始终读取当前登记文件，包括未提交修改，不读取 HEAD、index 或 Git 状态替代。
- 可选 remote 名/URL 是独立登记的身份约束，不是来源 URI 或获取指令。匹配按原始值和显式认可映射进行，SSH/HTTPS 不自动等价。

系统在采集或发布前拒绝未知来源类型、缺必填字段和不兼容字段组合；无效 command 字段不得启动进程，无效 Git 字段不得发布记录。

## 4. 观察与恢复 binding

来源版本保存完整原始内容、hash、长度、文本视图和原始 acquisition 描述。当前观察定义与历史恢复 binding 分开：

- `verify` 比较已记录完整旧内容和当前登记来源。
- `replace <commit-id>` 只修改该 commit 所引用完整 source version 的恢复 binding，并要求 `--expected`。
- replace 必须取得相同完整内容；相同片段不够。共享同一 version ID 的记录一起报告；内容 hash 相同但 version ID 不同的对象不混改。
- `gc --content` 只释放不再需要本地副本、且确切替代恢复依据仍可复核的内容。缺对象、损坏 binding、共享版本或 peer 保护都要保留内容。

hash 不能恢复旧内容。删除缓存或没有 Git 时，权威 metadata/content 仍必须足以恢复身份、历史和 Myers 所需基线。

## 5. 调用方观察与执行边界

`verify --json` 返回 `data.expected`。对已有 store/object 的写入必须通过 `--expected <JSON_OR_FILE>` 携带新的调用方依据。凭据绑定 publication、tips、来源版本/hash、项目/store/实例、映射和相关 peer/登记修订。

写锁内重新核对这些依据。来源、映射、登记或 publication 在观察后改变时，返回 typed conflict，不自动获取新凭据后重试。成功的新观察与原始 stale 依据是两件事：新观察可以授权对应新内容；旧依据仍必须失败且零发布。

## 6. JSON v2

结构化输出使用 `{schema_version, ok, data, diagnostics}`。ID 和大整数使用字符串，不适用字段为 `null`。范围查询分别报告：

- `node.kind` 与链根身份；
- `selected_commit_id`、`tip_commit_id`、`effective_range_commit_id`；
- `source_version_id` 与完整来源描述；
- 所属文件、项目相对路径和位置；
- link ID、创建 commit、对象端点和选定端点版本。

BEGIN/END 是结构标记，不携带新正文时仍可由有效范围版本取得正文依据。只有 BEGIN 时，有效范围版本为空、覆盖为零、闭合检查失败；这不是新标记状态。真正缺失来源版本仍明确失败，不用空正文或最新内容补齐。

## 7. 明确不做

- 不提供 OMD URI 或复合来源表达式语言。
- 不自动迁移旧 store、双格式读写、覆盖初始化或重写不可变历史。
- 不自动联网、同步副本、合并冲突或建立分布式锁。
- 不宣称 Myers 差异等于语义等价，也不判断理由是否合理或文档是否充分。
