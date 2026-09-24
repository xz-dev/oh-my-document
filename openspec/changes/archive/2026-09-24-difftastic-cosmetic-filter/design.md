## Context

动机见 [proposal.md](proposal.md)。现状：verify 以内容 hash 检测变化，用 Rust 内 Myers 比较完整旧新来源得出 dirty 范围；`src/relations/diff.rs` 已有 `diff_text`/`diff_bytes`/`dirtied_by`。三态标记模型（空/脏/已确认）与"诊断不是第四种状态"的边界已确立。外部命令执行的许可哲学已有先例：`--run-command` 一次性、不携带。用户已确认两个方向性决定：**壳调用 difftastic 二进制（不嵌入 tree-sitter）**；**语义是过滤而非列出**——复查队列按"先审结构变化、最后收尾 cosmetic"排序。

## Goals / Non-Goals

**Goals:**

- 格式风暴后，人/Agent 的注意力只落在结构树有变化的范围上。
- 过滤是可审计的：分类证据落库，收尾在锁下重分类。
- 库层零新增重依赖；difftastic 适配隔离在 CLI 层。
- Skill 同步教会 Agent"先过滤、后收尾、工具不可用诚实降级"。

**Non-Goals:**

- 不证明语义等价：结构无变化是工具判定，不是语义证明（R-05 不变）。
- 不新增第四种标记状态；分类是检查结果，不是标记生命周期。
- 不嵌入 tree-sitter 或任何每语言语法库；不做范围级分类（接口按文件级设计，留升级缝）。
- 不做下游 `clean` 批量阻断、不改 tag 系统（二期）。
- 不迁移既有数据、不升持久格式版本。

## Decisions

### D-1 壳调用 difftastic，库层定义 `DiffClassifier` 抽象

`omd` 库层新增 `DiffClassifier` trait（输入：旧完整内容 + 新完整内容 + 文件名提示；输出：`CosmeticOnly | Changed | Unclassified(reason)`）。CLI 层提供 difftastic 适配器：固定 argv 调用 `difft`，无隐式 shell。备选方案"嵌入 tree-sitter + 每语言语法"被否决：依赖扩张失控，且"支持哪些语言"变成政治问题；壳调用把语言支持留给 difftastic，OMD 只消费判定。代价是外部工具版本漂移——由 D-3 的证据钉住工具身份缓解。临时文件必须带真实扩展名（difftastic 靠扩展名选语法）。

### D-2 分类粒度：文件级起步，接口按范围级设计

分类对象是旧版本全文 vs 新全文（verify 已同时握有两者，零额外取数）。文件树有变化 → 该文件全部脏范围保守留在 dirty；树无变化 → 全部进 cosmetic。范围级映射（拿 difftastic 变化节点位置映射回范围）延后：范围切片常非完整语法单元，无法独立解析。备选"范围级"被否决因复杂度不成比例——文件级的失败方向是保守的（一处逻辑修改拖累同文件纯格式范围），可接受。

### D-3 证据与许可：`--difftastic` 即本次执行许可，判定证据随确认落库

参数本身即许可（同 `--run-command` 哲学），无配置层自动开启。`data.cosmetic` 桶在报告中携带工具身份与版本；`commit cosmetic` 提交时在锁下重算分类并把完整证据（工具、版本、旧/新版本 ID、判定）写入 commit payload。不做对象级 tag——"结构无变化"是**版本对**的属性，不是对象属性，做成 tag 会污染既有 tag 语义。报告分桶 + 确认提交携带证据已覆盖可审计性。

### D-4 退出码：dirty 桶独占失败语义

`--difftastic` 时 exit 1 仅由 `data.dirty`（结构变化 + 无法分类）决定；cosmetic 非空不设门禁。无过滤行为完全不变。unclassified 保守落 dirty：解析失败、byte 模式、超大文件、工具缺失/失败均不静默归入 cosmetic——宁可吵，不可静默。

### D-5 `commit cosmetic`：锁下重分类 + ATOMIC 批量续改

`commit cosmetic <path> --expected <凭据>`：进入写锁后重新分类锁内当前内容；与过滤视图不一致的范围拒绝续改、如实报告。通过者逐条续改，一个 ATOMIC 块发布，晚期 I/O 失败沿用既有部分发布契约。下游 link 待办保留，不做批量消除。备选"check --mark 一键标记"被否决：check 永不写。

**新坐标推导（已定案）**：cosmetic 的常态是范围内文本被重排、旧 fragment 逐字匹配不到（`locate_candidates` 返回 0）。新坐标**不用** difftastic 的行级 `aligned_lines`（行→Unicode 字符单位换算复杂且 range 可跨行任意切割），而是用**同一套 Myers hunks 做坐标映射**：`map_position`/`map_range`（`src/relations/diff.rs`）把旧 `[start,end)` 平移过 hunks 得到新 span。start 边界遇插入推到其后、end 边界遇插入不吞（与 `dirtied_by` 的 end-adjacent 规则对偶）。映射塌缩（end<=start）则拒绝续改、如实报告。

**证据 payload（已定案）**：分类证据写入 `payload.classification` 子对象（tool、version、language、status、chunk_count、comment_only、old_version_id、new_version_id）。`Commit` kind 的 closed payload 白名单新增 `classification` 键——仅 cosmetic 收尾路径写入，手输不提供入口。

### D-6 Skill 同步教学是交付的一部分

`skills/omd/SKILL.md` 复核章节改写为优先级队列：永远先 `--difftastic` 过滤 → 逐条审 dirty → 收尾批扫 cosmetic → 完成定义仍是无过滤 verify exit 0。三条纪律写死：先过滤、收尾不等于丢弃、工具不可用诚实降级为全人工。capabilities 分支同步更新。

## Risks / Trade-offs

- [difftastic 机器可读输出形态不稳定或回退到文本 diff 时难以与"真有变化"区分] → 实现前先做 spike 验证 `--display` 输出与 exit code 语义；不确定判定一律归 Unclassified。
- [外部二进制版本漂移导致分类结果不可复现] → 证据记录工具身份与版本；确认提交可追溯当时使用的工具。
- [用户把过滤视图当成完成信号，收尾步骤被跳过] → exit code 诚实（无过滤仍失败）+ skill 明确教"收尾不等于丢弃"；cosmetic 桶命名为待收尾而非安全。
- [文件级分类的保守性让混合文件（格式+逻辑）全留 dirty] → 有意取舍；范围级为二期升级缝。
- [超大文件或 difftastic 慢导致 verify 超时体验] → 未分类保守 + 如实报告原因；不引入静默跳过。

## Migration Plan

纯新增参数与命令；无持久格式变更、无数据迁移。回滚 = 不使用 `--difftastic` 与 `commit cosmetic`，一切行为回到现状。skill 更新为纯文档，独立于二进制回滚。

## Open Questions

- `commit cosmetic` 的分类证据具体 payload 字段布局（closed payload 字段规则的 schema 演进细节）——实现任务内决定，不影响本设计。
- difftastic 输出的解析方式（JSON vs 文本启发）——由实现前 spike 决定，失败方向已由 Unclassified 兜底。
