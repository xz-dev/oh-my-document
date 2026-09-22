# 范围身份、位置与关联：同类系统设计调研

调研日期：2026-09-21。OMD 对照版本：`0e5c04e`。

本文核对了需求追踪、文档标注、代码索引和静态分析领域的十个项目或规范。资料来自官方文档、协议和源码。**外部事实、OMD 已确认契约和候选建议分别列出；本文不授权修改存储格式或 CLI。**

## 1. 结论

用户提出的“是否应该拆开存”有充分参考依据，但应区分三个问题：

1. **内部模型：** 多个系统将资源位置与内容范围分成字段；需要持续追踪的实体通常另有身份标识。
2. **引用与序列化：** 结构化模型仍可提供字符串 ID、URI 或复合符号名。SCIP、Kythe 都有正式字符串文法，不能概括为“正确系统绝不用复合字符串”。
3. **持久化布局：** 分字段不等于必须分成多张数据库表、多个文件，更不意味着不能使用 key。`对象 ID → 结构化记录` 仍然可以是映射表。

对 OMD，关键是：**范围对象是谁、某一版本位于哪里、这次核对依据哪个版本，是三个不同问题。** 换掉 `@` 只能处理引用语法，不能代替对象身份设计。

## 2. 横向对照

| 项目／规范 | 已核实的数据模型 | 适用于 OMD 的启示与边界 |
| --- | --- | --- |
| **StrictDoc** | 节点有业务 `UID`、可选机器 `MID`；关系使用 `RELATIONS`。指南说明 MID 可在节点移动、UID 或标题变化时识别原节点。[1] | 可读名称与机器身份可以分工。但 MID 的稳定不证明所有按 UID 声明的关系都会自动修复。 |
| **Doorstop** | 每个条目存为 YAML；UID 取文件名去掉扩展名。`level`、正文、外部引用另存；链接记录目标 UID 和校验戳。[2] | 对象标识与被审查内容的指纹分开。也有文件名参与身份的设计，并非所有工具都完全解耦。 |
| **OMG ReqIF** | `SPEC-OBJECT`、`SPEC-HIERARCHY`、`SPEC-RELATION` 分别表达需求对象、层级位置、关系；关系有自己的 `IDENTIFIER` 和 `SOURCE`／`TARGET`。[3] | 对象、树中的位置、关系实例可以独立。规范是交换模型，不规定各产品内部数据库，也不保证任意导入操作都保留 ID。 |
| **W3C Web Annotation** | `Annotation.id` 与目标的 `source`、`selector`、`state` 分开；位置选择器有独立 `start`／`end`。[4] | 身份、位置、资源状态分层。位置字段仍会因编辑过期，拆开字段不等于解决重定位。 |
| **Hypothesis** | 实际数据库模型中，标注 `id` 是 UUID 主键；目标 URI、`target_selectors`、回复所引用的标注 ID 分别存储。[5] | 这是持久化实现证据，而不仅是 API 样例。多个标注无需把相同目标位置当成同一个标注。 |
| **LSP 3.18** | `Location = { uri, range }`；`LocationLink` 将 `targetUri`、`targetRange`、`targetSelectionRange` 分开。[6] | 路径和范围可直接用结构表达。不过 LSP 位置不是跨版本持久范围身份。 |
| **LSIF 0.6.0** | 文档、范围各有 vertex ID；范围保存 `start`／`end`，通过 `contains` edge 归属文档。[7] | 关系可以引用节点 ID，无需从字符串拆路径和坐标。其 ID 面向索引，不能自动视为跨索引稳定身份。 |
| **SCIP** | `Document.relative_path` 与 occurrences 分开；occurrence 有范围和 `symbol` 字段；symbol 同时有结构化模型和正式字符串文法。[8] | 结构化位置与规范化字符串身份可以共存。不能把语言符号标识直接当成 OMD 任意文本范围的生命周期身份。 |
| **Kythe** | `VName` 分 `signature/corpus/root/path/language`；anchor 的字节起止是独立 facts；另有 Kythe URI 文法。[9] | 数据模型与可传输字符串可以分层。注意 VName 本身包含 path，不能宣称其身份完全与路径无关。 |
| **SARIF 2.1.0** | `physicalLocation` 分 `artifactLocation` 与 `region`；结果另可带 `fingerprints`／`partialFingerprints`。[10] | 位置与跨运行结果识别的依据分开；指纹如何生成及其效果取决于工具，不能直接等同于 OMD commit。 |

以上系统的职责不同。LSP、LSIF、SCIP、Kythe 主要服务导航／索引；StrictDoc、Doorstop、ReqIF 服务需求关系；Web Annotation、Hypothesis 服务标注。这里比较的是身份与位置的建模方式，不是宣称它们都实现了 OMD 的生命周期。

## 3. 几个直接回答问题的实例

### 3.1 LSP：资源与范围直接分字段

LSP 的 `Location` 定义为：[6]

```typescript
interface Location {
    uri: DocumentUri;
    range: Range;
}
```

URI 是一个字段，起止位置在另一个结构中。文件名字符不会被当成范围分隔符。

但 URI 本身仍须遵守 URI 编码。LSP 的行内偏移还涉及协商的 UTF-8／UTF-16／UTF-32 单位，不能未经转换替代 OMD 已确认的 Unicode 字符序号／原始字节坐标。

### 3.2 Hypothesis：数据库确实分开存

`h/models/annotation.py` 中：[5]

- `id`：`types.URLSafeUUID` 主键。
- `_target_uri`：目标资源 URI。
- `target_selectors`：独立 JSONB 选择器列。
- `references`：所回复的祖先标注 ID 数组。
- `target` 属性将 URI 与选择器组装成 API 返回值。

这直接支持“对象 ID 不必包含路径和范围”的设计。这里未审核其完整客户端重锚定算法，不据此承诺删除、移动或任意改写后的定位成功率。

### 3.3 ReqIF：对象、层级位置、关系也是三件事

官方 XSD 明确区分：[3]

- `SPEC-OBJECT.IDENTIFIER`：需求对象身份。
- `SPEC-HIERARCHY.OBJECT`：引用需求对象，层级孩子另在 `CHILDREN` 中表达。
- `SPEC-RELATION.IDENTIFIER`：关系本身的身份。
- `SPEC-RELATION.SOURCE/TARGET`：两端对象引用。

因此，模型允许重新组织层级而不必改变需求对象 ID；具体工具是否这样操作仍由实现决定。不能从 XSD 推导“移动时任何工具都绝不会断链”。

### 3.4 Doorstop：端点是谁与核对过哪个内容分开

实际源码中：[2]

- `Item.uid` 从 `basename(path)` 去掉扩展名得到。
- `Item.stamp()` 对 UID、正文、引用等审查相关内容计算校验戳。
- 检查链接时比较所记录的 `uid.stamp` 与目标当前的 `item.stamp()`。
- `clear()` 更新指定父条目的校验戳。

这与 OMD 的“关联持续存在，但所依据的内容需要重新核对”有参考关系。不过 Doorstop 的链接集合行为不能替代 OMD 已确认的“相同端点可有多个独立 link”。

**边界修正：** UID 来自文件名，不是完整目录路径。仅凭上述源码不能断言“移动到任何目录都必然改变 UID”，也不能断言工具提供的移动操作一定不会更新引用。

### 3.5 W3C：位置不是身份，也不是充分的版本依据

Web Annotation 将目标来源、选择器和状态分开：[4]

```json
{
  "id": "http://example.org/anno24",
  "type": "Annotation",
  "body": "http://example.org/review1",
  "target": {
    "source": "http://example.org/ebook1",
    "selector": {
      "type": "TextPositionSelector",
      "start": 412,
      "end": 795
    }
  }
}
```

规范明确警告，位置选择器对资源修改很脆弱，并建议额外使用 `State` 标识适当的资源表示。它还提供 `TextQuoteSelector` 的 `exact/prefix/suffix` 和 `DataPositionSelector` 字节范围。

这不是强制所有实现同时使用“位置＋引文＋快照”的算法，更不保证模糊匹配正确。OMD 的 Myers、版本依据和歧义处理仍须遵守自己的契约。

### 3.6 反例：字符串本身不是错误

SCIP 的 symbol 有明确语法和转义规则；Kythe URI 是结构化 VName 的文本编码；Web Annotation 也允许片段 IRI，同时解释其局限并推荐 SpecificResource／FragmentSelector。[4][8][9]

所以：

- 文件名允许 `@`，**不能证明所有使用 `@` 的文法都必然歧义**；完整文法、转义或明确的字段边界可以实现可逆编码。
- shell 引号只解决 shell 参数边界，不能修复 OMD 内部 `find('@')` 的语义。
- 无歧义序列化也不能证明“路径＋坐标”适合充当持续对象的身份。
- length-prefix、JSON、规范 URI 等各有用途，length-prefix 不是唯一正确方式。

## 4. 对照 OMD：哪些早已确认，哪些是当前实现

### 已确认的产品契约

[design.md](../../openspec/changes/define-tracking-contracts/design.md) 的 **D-17** 明确：

- 相同坐标可以创建多个独立范围链。
- 通过已有 commit 续改范围时，坐标可以改变。
- 范围身份不是坐标，而是从首个 range commit 开始的提交链。

**D-24** 进一步区分：

- range 身份与它的提交版本。
- link 自己的持久身份与两个范围端点。
- 持续关联与操作／复核所依据的具体版本。

这些不是本次调研才提出的新需求。范围 ID 的具体表示与 CLI 拼写尚需工程设计，但不该重新让用户在“坐标是否就是身份”上做选择。

### 当前源码事实

本次局部检查发现：

- [`src/relations/range.rs`](../../src/relations/range.rs) 已有结构化 `Range { start, end, mode }`。
- [`src/records/store.rs`](../../src/records/store.rs) 已有独立 `link_id`；但 `tips`、`mounts`、`dirty` 和 link 的 `source/target` 等仍使用字符串节点 key。
- [`src/relations/node.rs`](../../src/relations/node.rs) 将 key 生成为 `range:<path>@<mode>:<start>-<end>`，同坐标独立链通过 `#nonce` 区分。
- [`src/main.rs`](../../src/main.rs) 的 `--id` 路径会查找所属链；部分其他功能又通过 key 拆解路径或范围。更新 payload 中的范围，与解析 key 中的范围，是两条需要一致性审查的路径。
- [`src/records/binding.rs`](../../src/records/binding.rs) 已将来源版本 ID 与 acquisition binding 分开。

因此不能说“OMD 所有信息都只在一个 key 里”，也不能说“已有实现必然无法区分同坐标对象”：nonce 已经处理了后一种情况。

真正需要纠正的是：**同一个字符串既充当节点身份，又被多处当成可拆解的位置数据。** 在坐标可修改、路径可变、同坐标对象可重复的契约下，这会增加一致性负担。含 `@` 路径的失败是已观察到的一例；其他生命周期路径尚未在本轮全面复测。

README 示例中的 `version record missing` 是另一条已记录问题。本轮没有证明它与身份建模同根，不合并归因。

## 5. 建议：沿已有契约分工，不先发明新分隔符

以下为建议，尚未实施或冻结 schema。

### 5.1 分开五种信息

| 信息 | 回答的问题 | 候选表达 |
| --- | --- | --- |
| 范围对象身份 | 这是不是原来的那个跟踪对象？ | 既有范围链身份；优先核对能否复用首个范围 commit 的身份 |
| 来源定位 | 内容从哪里取得？ | 结构化 acquisition：文件路径、项目来源或固定命令及 argv |
| 范围位置 | 某个版本中选了哪些内容？ | `mode/start/end` 字段 |
| 版本依据 | 这次操作与复核用了哪个内容？ | 原有 commit／source-version 引用 |
| 关联实例 | 正在处理哪一条关系？ | 已有独立 `link_id`，两端引用范围对象 |

不默认再引入一套 UUID。先复用既有身份与版本体系，并核对首 commit、reset、占位记录和 GC 的生命周期约束。

概念上可以这样分开；**不是最终存储格式，也不是已存在的 CLI 输入**：

```text
范围身份       = { store_id, range_chain_identity }
范围版本       = { commit_id, range_identity, source_version_id,
                   mode, start, end }
来源获取描述   = { kind: file, path: "we@ird.md" }
关联实例       = { link_id, source_range_identity, target_range_identity }
```

文件与 command 都是来源，不能为了修复文件路径引用而丢掉命令输出的跟踪能力。关联端点继续代表范围对象；某次适配使用哪个版本，仍单独记录，不用当前最新版本偷偷替换旧依据。

这些分工可放进现有结构化文本记录，不要求新增数据库、ORM、图数据库，或按每个概念新建一个文件。

### 5.2 CLI 的创建、查找与引用分别处理

- **创建范围：** 来源与范围参数可以分开。原来的路径参数与 `--range` 已经是这种形式。
- **引用已有范围：** 优先考虑从已有 commit／链引用解析到确定的范围身份，保留可重复的 `--link-from`／`--link-to`。
- **按路径和坐标查找：** 它是选择条件，不天然是唯一对象 ID。同坐标有多个对象时，必须明确消歧，不静默挑一个。
- **跨存储引用：** 在内部明确区分 store 与范围身份；对外如何编码再统一设计。
- **便捷字符串：** 若确实需要，再为结构化模型定义一套可逆、可验证的文本表示。不能反过来由拼接格式决定对象模型。

参数分离不等于“每种属性只能有一个全局 flag”。重复参数可以按组解析，或者引用现成 ID。因此，之前关于“参数拆开必然破坏多 link”的结论不成立。

“统一”应指所有入口最终解析为同一种引用对象、遵守相同校验，不要求创建对象和引用对象使用完全相同的文本拼写。

### 5.3 后续设计必须过的行为检查

1. 同来源、同模式、同起止创建 A 和 B：两者独立，查询坐标不能偷偷合并。
2. 对 A 提交新坐标：A 的身份及已有 link 不因此变成另一个对象；旧版本坐标可追溯。
3. 显式改名／移动来源：位置变化与对象身份如何保持，须按生命周期契约处理，不靠字符串替换猜测。
4. A 到 B 存在 L1、L2：处理 L1 不处理 L2，端点相同不合并关系。
5. 跨 store：局部身份相同也不应误指向另一个 store；失联不能自动匹配同名文件。
6. 支持的合法路径包含 `@`、`#`、括号、冒号、空格或类似 `text:0-9` 的片段时，不改变字段边界。
7. 位置相同但版本依据不同：保留各自证据，不因对象存在就自动确认或替换旧 hash。
8. 定位失败与歧义：报告实际诊断，不创造新的标记生命周期，不误报锁冲突。

这些是后续要验证的行为，不是本次已执行的测试。非 UTF-8 操作系统路径字节等另有编码契约，本调研不据此扩大支持承诺。

## 6. 核验范围与剩余事项

- 阅读规范／源码和 OMD 的相关设计条目；未运行同类项目的完整产品、数据库迁移或重定位性能测试。
- 使用代码图定位 OMD 符号并核对源码。Serena 当前无可用语言服务器，本轮未配置或安装它。
- 复核并删改初稿中的过度推论：Doorstop 任意目录移动必然改 UID、W3C 强制组合选择器与自动重锚定、ReqIF 保证所有工具移动不破坏关系、OMD 同坐标必然发生 key 冲突。
- 下一步应形成与 D-17／D-24 一致的最小模型和 CLI 提案。具体 schema、旧数据处置、CLI 编码须得到确认后再实施；研究不等于迁移授权。

## 一手来源

部分 URL 跟随上游分支；以下注明查阅的文档版本，源码可固定的已固定。检索结果不作为字段事实的依据。

[1] StrictDoc 官方指南：[MID / UID](https://strictdoc.readthedocs.io/en/stable/stable/docs/strictdoc_01_user_guide.html#SECTION-UG-Machine-identifiers-MID)；[文档源码](https://raw.githubusercontent.com/strictdoc-project/strictdoc/main/docs/strictdoc_01_user_guide.sdoc)，查阅 `SECTION-UG-Machine-identifiers-MID`、`SDOC_UG_REQUIREMENT_RELATIONS`、代码追溯章节。MID 的主要证据为源码约 1426–1437 行。网页已成功读取；后续用另一 HTTP 客户端重读遇到 403，未绕过访问控制，后续核验使用公开文档源码。

[2] Doorstop：[Item 实现](https://github.com/doorstop-dev/doorstop/blob/develop/doorstop/core/item.py)，`uid`、`links`、`cleared`、`stamp`、`clear`；[UID / Stamp 类型](https://github.com/doorstop-dev/doorstop/blob/develop/doorstop/core/types.py)。`uid` 取 basename 的证据约 422–424 行，校验戳比较约 530 行，`stamp/clear` 约 864–884 行。

[3] OMG [ReqIF 规范入口](https://www.omg.org/spec/ReqIF/)；[官方 reqif.xsd](https://www.omg.org/spec/ReqIF/20110401/reqif.xsd)，直接核对 complexType `SPEC-OBJECT`、`SPEC-HIERARCHY`、`SPEC-RELATION`，`IDENTIFIER` 为必需的 `xsd:ID`。规范不要求这些 ID 必须是 UUID。

[4] W3C [Web Annotation Data Model，2017-02-23 Recommendation](https://www.w3.org/TR/2017/REC-annotation-model-20170223/)：Specific Resources、Fragment IRIs、Text Quote Selector、Text Position Selector、Data Position Selector、States；[当前位置选择器章节](https://www.w3.org/TR/annotation-model/#text-position-selector)。

[5] Hypothesis：[后端 Annotation 模型](https://github.com/hypothesis/h/blob/main/h/models/annotation.py)，查阅 `id`、`_target_uri`、`target_selectors`、`references`、`target`。UUID 主键约 52–54 行，selector 和 ancestor IDs 约 111–120 行，API target 组装约 196–205 行。本文关于持久化的结论以此源码为依据，不依赖 API 文档推测内部实现。

[6] Microsoft [LSP 3.18 — Location](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/#location)、[LocationLink](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/#locationLink)、[Position](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/#position)。

[7] Microsoft [LSIF 0.6.0 — Ranges](https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/#ranges)、[Result ranges](https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/#resultRanges)。普通 range 的不重叠约束与 resultRange 的用途有区别，不能照搬为 OMD 的范围限制。

[8] Sourcegraph [scip.proto](https://github.com/sourcegraph/scip/blob/e01e97efac2f6b8c266b4d04825f1f1eab7b8f6c/scip.proto)，固定源码版本 `e01e97e`：`Document`、`Symbol` 文法、`Occurrence`。该版本 `Occurrence` 有 typed range，并保留已弃用的整数数组 range；结论只依赖“范围与 symbol 分字段”，不绑定旧数组编码。

[9] Kythe [storage.proto](https://github.com/kythe/kythe/blob/811e2dc5e2b1d6e37e3d7172bae0132c5ec469df/kythe/proto/storage.proto)，固定源码版本 `811e2dc`，`VName` / `Entry`；[anchor schema](https://kythe.io/docs/schema/#anchor)，`loc/start`、`loc/end`；[Kythe URI 文法](https://kythe.io/docs/kythe-uri-spec.html)。

[10] OASIS [SARIF 2.1.0 schema](https://github.com/oasis-tcs/sarif-spec/blob/a560296ca8c921f3bdb8d4a8db57ab83dae968a7/sarif-2.1/schema/sarif-schema-2.1.0.json)，固定源码版本 `a560296`：`physicalLocation`、`artifactLocation`、`region`、`result.fingerprints` 和 `result.partialFingerprints`。
