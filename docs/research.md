# 调研依据与能力边界

本页记录支持设计讨论的公开一手资料，不是同类产品全面市场调查、性能基准或实现验证。用户决定来自 [requirements.md](requirements.md)，外部资料用于校准能力边界，不能替代用户决定。

来源索引为 [research/sources.json](research/sources.json)。URL 多为上游主分支/最新文档，并未冻结依赖版本；后续选库应重新验证具体版本。此页没有把资料中的示例命令当作安装或执行授权。

## OpenSpec 与 Mermaid：推荐的外围创作工具

OpenSpec 以 proposal、spec、design、tasks 等文本产物组织变更，并强调可迭代工作，而不是不可回退的刚性阶段。[1][2]

Mermaid flowchart 使用文本定义节点、边和子图；mermaid-cli 可从定义文件产生 SVG、PNG、PDF，并支持 Markdown 中的 Mermaid 块。[3][7][8]

**对 OMD 的启示：** 可以推荐这些文本来源，OMD 自己负责链接、跟踪与规则检查。不要把“推荐”变成核心依赖，也不要把所有 Mermaid 图都等同于正式 UML 元模型。此次没有执行图渲染或初始化 OpenSpec。

## Myers、Git 与空白

Git 的 diff 文档提供 Myers/default、minimal、patience、histogram 等算法选择；忽略空白的选项与移动 diff hunk 边界的 indent heuristic 是分别列出的机制。[5]

**对 OMD 的启示：** 借用 Myers 算法不等于读取 `git diff`。不能从“使用 Myers”推导“会安全忽略格式化”或“匹配就是对象身份”。OMD 已明确不依赖 Git；这里引用 Git 文档是解释算法家族与选项边界，不是新增依赖。

Git 的 pre-commit 可用非零退出阻止提交，但也可被 `--no-verify` 绕过。[4]

**对 OMD 的启示：** 可选 hook 适合防忘记，不是不可绕过的权限边界。没有因这一事实额外批准服务端门禁或组织审批系统。

## 文本选择器

W3C Web Annotation Data Model 描述了 TextPositionSelector 的位置选择，以及 TextQuoteSelector 用 exact/prefix/suffix 描述文本片段；标准也指出仅位置选择对文本修改较脆弱，并建议结合资源状态。[6]

**对 OMD 的启示：** 范围应绑定明确内容依据；位置与身份不能混为一谈。该标准只是参考，未决定采用 JSON-LD、其归一化规则或多匹配处理方式。OMD 的文本 Unicode 字符计数和显式 byte 模式以用户决定为准。

## SQLite 与 Git 友好的记录

SQLite 官方列出本地应用文件格式等适用场景；数据库文件是页式结构，事务/恢复还可能涉及 journal 或 WAL。[9][11]

Git 文档说明 textconv 可把二进制转成便于阅读的差异，但这种差异不能直接用于应用补丁；其 binary merge 驱动也不会替应用合并数据库行。[10]

**对 OMD 的启示：** 用户接受了文本权威记录 + SQLite 可重建索引，而非运行中的 SQLite 直接作为 Git 内权威记录。本次没有做数据库选型基准；基线二进制快照与可审阅元数据仍需分开设计。

## 配置与缓存目录

XDG Base Directory Specification 定义配置与缓存环境变量以及未设置/为空时的默认目录，并要求其自身路径变量为绝对路径。[12]

Rust 的 directories::ProjectDirs 提供 Linux、Windows、macOS 应用级配置/缓存路径计算，并遵循对应平台惯例。[13]

**对 OMD 的启示：** 平台目录库只作为后备解析。用户已明确 `OMD_CONFIG_PATH`、`OMD_CACHE_PATH` 高于 XDG；XDG 自身变量的限制不能未经决定套用给 OMD 专用变量。尚未选择该 Rust crate 的版本或实际加入依赖。

## 未完成的验证

没有验证未来 OMD 代码、实际 Rust crate 的 Myers 行为、二进制大规模差异性能、命令跨平台执行、缓存重建或源文档语义。以上均留给后续实现与真实测试；不能从阅读文档直接宣称它们已经工作。

## Sources

[1] https://raw.githubusercontent.com/Fission-AI/OpenSpec/main/README.md — OpenSpec README
[2] https://raw.githubusercontent.com/Fission-AI/OpenSpec/main/docs/getting-started.md — OpenSpec Getting Started
[3] https://mermaid.js.org/syntax/flowchart.html — Mermaid Flowcharts Syntax
[4] https://git-scm.com/docs/githooks — Git githooks
[5] https://git-scm.com/docs/git-diff — Git git-diff
[6] https://www.w3.org/TR/annotation-model — W3C Web Annotation Data Model
[7] https://github.com/mermaid-js/mermaid-cli — mermaid-cli repository
[8] https://raw.githubusercontent.com/mermaid-js/mermaid-cli/master/README.md — mermaid-cli README
[9] https://www.sqlite.org/fileformat.html — SQLite Database File Format
[10] https://git-scm.com/docs/gitattributes — Git gitattributes
[11] https://www.sqlite.org/whentouse.html — Appropriate Uses For SQLite
[12] https://specifications.freedesktop.org/basedir/latest — XDG Base Directory Specification
[13] https://docs.rs/directories/latest/directories/struct.ProjectDirs.html — directories::ProjectDirs Rust documentation
