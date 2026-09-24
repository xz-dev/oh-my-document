# Spike: difftastic 可用性探测

日期：2026-09-24 | 版本：`Difftastic 0.69.0` (emerge dev-util/difftastic-0.69.0)

## 判定路径

`--exit-code` 是唯一可靠的机器信号：

| 场景 | exit | 判定 |
|---|---|---|
| 相同文件 | 0 | CosmeticOnly |
| 仅空白/格式变化 | 0 | CosmeticOnly |
| 逻辑修改 | 1 | Changed |
| 新增/删除行 | 1 | Changed |
| 缺失文件 | 2 | Unclassified |

`--check-only` 打印 "No syntactic changes." / "Has syntactic changes." 但 exit 恒为 0——**不要用**。

`--display json` 需要 `DFT_UNSTABLE=yes` 环境变量，且输出为不稳定格式（官方明确警告）。`status` 字段值为 `"unchanged"`/`"changed"`；注释变化判 `"changed"`；解析失败回退为 `"Text (... parse error ...)"` 且仍报 `"changed"`。JSON 可解析但不稳定，**不作为主信号**，只作证据记录。

## 关键发现

- **语法回退语义**：无法解析扩展名 → `language:"Text"`，按文本 diff。文本 diff 对格式变化也会报 `changed`。
- **文档不对称（重要）**：`difft --list-languages` 中**没有 Markdown**——`.md` 文件全部走 Text 回退，纯空白变化也报 `changed`。含义：本工具自家 `.omd` 存储（跟踪的几乎全是 `.md`）**不会**从该特性获得 cosmetic 过滤收益；受益面是代码文件（`.rs`/`.py`/`.json` 等有语法的）。语义上仍保守（宁可吵），但 skill 与文档必须如实写明此边界，防止 Agent 带着"过滤了格式风暴"的错误预期工作。
- **注释变化**：报 `changed`（exit 1），但 chunk `highlight:"comment"` 可识别。`--ignore-comments` 可使纯注释改动 exit 0——**不采用**：注释对人判断有意义，默认保留在 dirty；该类别作为证据字段传递。
- **环境变量污染**：`DFT_EXIT_CODE`、`DFT_OVERRIDE`、`DFT_BYTE_LIMIT` 等都会改变语义 → 适配器必须 `env_clear()`，只显式传 `DFT_UNSTABLE=yes`。
- **目录 diff JSON 是数组**；单文件 diff JSON 是单行对象（NDJSON 形态）。`status` 有 `unchanged`/`changed`/`created`/`removed` 等。
- **二进制文件**：按文本 diff，随机字节变化 → `changed`。byte 模式范围在 OMD 层直接归 Unclassified，不送 difft。
- **超大文件**：`DFT_BYTE_LIMIT`（默认 1MB）/`DFT_GRAPH_LIMIT` 超限回退文本 diff——同语法回退，报 `changed`，保守。
- **argv**：`difft --exit-code <old> <new>`，固定两参数，无隐式 shell。临时文件必须带真实扩展名。

## 用户决定（2026-09-24 后续）

**全喂、取交集**：文本文件不预筛，全部喂给 difftastic；difftastic 能出结构化视图（JSON chunks）就把它作为证据给人/Agent 看；出不了（进程失败 exit 2、JSON 解析失败）就用 Myers 视图，标注 `difftastic_unavailable`。没有"该跳过哪个文件"的预筛规则——difftastic 自己的 Text 回退已经覆盖了未知扩展名场景（虽然 Text 模式下空白也报 changed，但该范围仍如实落在 dirty，只是视图来源标注清楚）。
