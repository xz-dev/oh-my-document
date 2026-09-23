# oh-my-document：AI 协作与交接说明

## 当前状态与阅读顺序

仓库已有 Rust 核心、`omd` CLI、BDD/集成测试和 OpenSpec change；整体 change 仍待最终独立验收。已有仓库内 OMD skill 首版及真实自管理存储，尚未全局安装或完成全部功能实践；未建立或运行 Lean 产品证明。

接手顺序：

1. [交接说明](docs/handoff.md)：当前实现、证据与剩余限制。
2. 当前 OpenSpec change：`openspec/changes/separate-range-identity-and-location/`。
3. [规格与任务证据](spec-traceability.md)：24 项任务的实现/测试对应。
4. [来源与坐标](docs/source-model.md)、[存储与路径](docs/storage.md)：当前接口和持久化边界。
5. [需求基线](docs/requirements.md)、[待决事项](docs/open-questions.md)：历史决定；其中复合来源语法已被当前 change 取代。
6. [programming-thinking 契约](docs/programming-thinking.md)：可选 Lean skill 的使用边界，不是已交付 skill。

## 本仓库的 OMD 自管理

修改已跟踪内容、创建关联或检查覆盖时，先读 [OMD skill](skills/omd/SKILL.md) 和 [当前纳管范围与实践限制](docs/omd-self-management.md)。沿用现有 `.omd/`；串行写入，每次取得新的调用方凭据。缺口如实保留，不重新初始化权威或批量确认全仓库。

## 保持当前产品边界

- 产品是 Rust 核心 + CLI + 指导 AI 的 skills。人和 AI 使用同一套规则，不设宽松通道。
- 核心用内容 hash 检测变化，在 Rust 内用 Myers 定位差异；当前文件观察不依赖 Git diff、changed-files、HEAD、index 或 blob。
- 标记只有空、脏、已确认三态；执行失败、无法定位和跳过是诊断/检查结果，不是第四种标记状态。
- 只有显式 import 的范围纳入统计；允许显式 import `.omd/`。跟随符号链接并报告断链/循环。
- 禁止并发更新。写入必须携带新的调用方观察凭据；依据过期即拒绝，不自动重试、合并或覆盖。
- OpenSpec、Mermaid 和 Lean 都不是核心运行时强制依赖。富文档转换在外部完成，不新增 DOCX 内部解析器。

## 使用结构化来源和对象引用

当前 CLI 不使用旧 `proj:...`、`command::...`、`git::...`、`--source-ref`、`--source-json` 或 `path@span` 输入。

- 范围：独立路径 + `--range <start> <end>` + `--mode text|byte`。
- 对象：本地 commit ID；跨 store 端点为每项固定的 `<alias> <commit-id>`。链根是对象身份，tip、有效范围版本、来源版本和 link ID 分开。
- file：默认来源；其他项目的恢复来源用 `--source-project`、`--source-path`。
- command：`--source-type command --executable <program> --args-json '<JSON 字符串数组>'`。固定 argv，stdout，exit 0；无隐式 shell、变量模板或编排语言。
- Git：`--source-type git --source-project <alias> --git-commit <完整对象 ID> --git-path <提交内路径>`。只读本地现有对象，不 clone/fetch，不接受浮动 ref。
- 文本范围按实际编码解码后的 Unicode scalar value 计数；byte 范围按原始字节计数。两者都是 0 起点、左闭右开；BOM、CRLF 和原字节保留。
- 读取配置、查看历史、查询、tree/log 或重建缓存不得执行 command。显式 init/replace 只采集一次；verify/check 依许可运行。

## 保持存储和定位边界

- `.omd/` 或显式外置 metadata 是权威；缓存索引可删重建，不能保存唯一理由或唯一基线。
- 先尊重 `OMD_CONFIG_PATH`、`OMD_CACHE_PATH`，再用 XDG/平台后备。显式错误路径不得回退。
- 本机 `projects.toml` 保存 alias 到 project root/metadata root 的位置；共享记录不保存个人绝对路径。
- 同一权威 store 的移动通过显式重定位保留 store ID。复制成另一可写权威必须显式 activate，取得新 store ID，并完成必要 peer 保护；旧外部引用不自动换绑。
- 真正新项目可先只有默认 `.omd/omd.toml` 编码配置再首次 init；已有、损坏、外置或已绑定目标不得借此覆盖初始化。
- 不支持的持久格式明确拒绝且保持原字节，不做迁移、双格式读写或自动修复。

## programming-thinking 边界

- 仅用于已有 UML 源码细节中的顺序状态机、逻辑链和程序性遗漏；先写证明义务，再做伪代码级模型和实际证明。
- 不用于并发、多线程、调度、锁或线程交错，也不能据此声称验证 OMD 单写者实现。
- 经证明单元必须与实际实现逐分支、顺序、状态和错误路径对应；逻辑改变先同步 UML/Lean 并重证。
- Lean 必须由 OMD 直接 link 到实际 UML 源码范围和实现范围。分别报告模型证明、实现对应审查、实现测试和 link 检查。
- 当前仓库只有契约文档；不要声称已安装 skill、建立 Lean 工程或完成证明。

## 修改与验证

- 已确认要求和当前 change 优先于旧设计文档；旧复合来源示例只能作为明确标记的历史/拒绝示例。
- 不用绿色总数、handoff 声明或任务勾选替代行为证据。每项完成必须指出实际源代码、可失败断言、命令结果和限制。
- 不承诺 Myers 忽略所有格式化、证明语义等价、判断理由合理或保证文档充分。
- 变更实现或契约时同步 README、受影响文档和 `spec-traceability.md`；不得让范围迁移、脏传播、确认作用域、执行许可或副本权威由代码偶然决定。
- 未经明确授权，不安装 hooks、发布版本、提交/推送 Git、归档 OpenSpec change、选择许可证或修改真实用户配置/存储。
