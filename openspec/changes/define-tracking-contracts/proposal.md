## Why

现有文档已经确定 OMD 的产品边界，但范围复核、变更清理、命令采集和跨项目身份仍缺少足以唯一决定程序行为的契约。将逐轮讨论中已确认的决定及时保存，并与未决细节分开，避免后续实现凭猜测补齐或遗失用户修正。

## What Changes

- 范围仅发生位置变化时，提供候选迁移并要求显式复核；允许选用固定英文理由，不自动推断语义无变化。
- 显式 import 的目录持续计入覆盖率统计范围，范围内新文件呈现覆盖缺口；import/remove 只为统计百分比和打 tag 服务，不直接管理文件或 range，文件的跟踪靠用户对具体文件的显式 init/commit。
- 项目根仅用于挂载本项目文件节点，挂载树与提交的适配引用分开。文件节点从只有 `path.target`、省略 `path.source` 的初始化 commit（如 B0）开始；初始化只登记文件自身，不与上游适配合并，上游适配在初始化之后的适配 commit 中记录。重命名以含 source/target 的迁移 commit 记录。支持表达 B 到 C 后 A 到 B 的路径复用，不将同名路径下的不同对象历史混为一谈；迁移执行和关联处理细节见 D-15。
- 虚拟全局 root 是 OMD 的唯一根，用于关联所有本 OMD 关联的项目（如 git 仓库）。层级为：虚拟全局 root → 项目根 → 文件节点。项目根通过 add project commit 加入；import commit 挂在 project root 下，是一个不管理文件 commit 的单独分支，用于统计覆盖与打 tag。tombstone 删除是 `path.target` 为 null 的迁移 commit，同路径重建为新节点。
- 迁移 commit 由用户手动 commit 产生，verify 不自动检测改名；它只确认最新 commit 指向的文件在磁盘上真实存在且 hash 匹配。复制产生新节点，不自动记录来源或建立关联。删除通过 tombstone commit 记录，区分有意删除与文件丢失。
- 迁移（rename）后原节点上的已确认 range commit 保持确认，link 关系不变，仅 path 从 source 变 target；迁移不产生重新 diff。note 只增不改不删，修改/删除是在同一 note id 上追加新 note（patch/delete note）。dangling commit 的长期保留策略是显式 gc 命令。reset 目标语法仅完整 commit ID 或唯一前缀，不提供父提交简写。
- 树到 range 层面：文件节点下挂字符/字节 range 跟踪节点，只有 range 节点会变脏，文件 commit 节点本身不变脏。文件 hash 变化后，verify 用 Myers 只标脏受影响的 range；用户逐个解决后 `commit verify <path>` 产生记录新 hash 的文件 commit。注册的 command 是虚拟文件对象，同样由 commit 初始化，输出变化即变脏。悬空提交的“重放”是用户查看后手工重新应用，不是 OMD 自动操作。
- 区分 OMD `commit --reason …` 与 `commit clean`：实际适配提交接住上游影响并继续处理下游；clean 在源端显式阻断所选分支，不是提交后的完成盖章。clean 默认提供理由，允许用 `--no--reason` 显式省略理由。`verify all cleaned` 检查是否仍有未处理责任，不要求每条关系额外调用 clean。
- 一次适配 commit 可明确选择一个、多个或全部上游变化；未选中的影响继续保留，不能因下游有新提交而全部清空。
- 增加 `commit unclean` 显式叠加脏状态，即使内容未变也必须处理；`reset` 可作用于除虚拟全局 root 外的任意 commit，将有效状态移到目标 commit 本身，保留目标，后续提交退出当前有效历史成为 dangling。不追加反向提交、不改源文件，也不因 reset 本身删除旧提交和 note；上层 commit reset 是否级联到子节点、长期清理策略仍待细化。
- reset 后，直接或间接必要依据中存在从当前有效根不可达提交的节点，在 verify 中报告 **unreachable_link**（broken 的一种，dirty 诊断原因），不能因数据仍在磁盘或直接引用可索引而通过。诊断展示断裂路径，并在存在合格目标时给出经检查可用的回退 commit ID；使用者先 reset 到依据完整可达的提交，再重新 commit。普通适配或 clean 不能绕过该修复，也不自动回退其他节点。
- 悬空（dangling）本身与 unreachable_link 区分：dangling 是数据仍在但已退出有效历史；unreachable_link 是因 dangling 关系而 broken、需要修复的状态。两者都不是第四种标记生命周期。
- 迁移延续同一节点身份：reset 可沿迁移链选择目标，不受 target 路径是否仍是当前磁盘位置限制。迁移 commit 含 path.source、path.target 与 reason，均参与 hash。verify 还会检测多个节点叶子指向同一文件路径的 **conflict** 状态（broken 的一种）。
- 提供指定存储范围内悬空 commit 的列举（`omd list --dangling`）、按 ID 查阅，以及用户显式选择的重放或清理。reset/verify 不自动重放或清理；清理通过显式 gc 命令由用户主动触发，不自动到期。
- 每次新 commit 都生成独立 commit ID，包括内容修改、clean 和 unclean。16 位随机字符串盐、本节点上一个 commit ID（首条为空字符串）、时间戳、本地内容和完整不可变操作输入参与 SHA-256，盐固定在最前。已有提交复制或跨项目引用时保留原始输入与 ID，不重新生成；project_id 保留为首次纳管时生成并持久化的项目登记元数据，不进入 hash，也不通过操作载荷间接加入。
- OMD 本身是 Git 样式状态数据库，区块链只作前驱哈希 ID 的参考，不引入 block/head、共识、挖矿或分布式账本架构；提交身份与存放位置、项目登记信息分开。
- 独立 `note` 子命令将多条评论外挂到指定 commit，以平面有序数组组织，不做回复树。note 是单独的数据存储（note 内容 + 时间 + commit ID），可对任意 commit 进行，任意 commit 可反查 note list。评论和 reset 不改变历史 commit ID，reset 后的新提交不复用旧 ID。每个 commit 都记录时间。
- link 只连接 range commit 到 range commit，不能直接 link 文件到文件；link 是 commit 下的子命令。脏状态按 commit 的 range 判定而非按位置：diff 命中的行使覆盖它的相关 commit 全部变脏，用户可用 `commit clean --no--reason` 取消并用阻断阻止传播。没有独立的 Myers 基线存储，Myers 只是 diff 时的算法。range commit 直接由 `commit --range` 初始化：无 `--id` 新建（可重叠、同坐标可并行多个），有 `--id` 在既有 commit 基础上续改并可修改范围；range 节点的身份是 commit 链而非坐标。
- import / remove / delete / init 都是 commit 的别名（`commit import` / `commit remove` / `commit delete` / `commit init`）；remove 对应 import 且只退出覆盖率统计整体（不删除已在跟踪的内容），delete 对应 init。dangling 是状态不是命令，列出悬空 commit 的入口是 `omd list --dangling`。编码由用户 CLI 参数或配置指定；引用文法路径只用 `/`；空文件覆盖率显示 100%。
- 规则是命名的检查集合：用 `commit tag` 给文件夹或文件打 tag（如 spec），再用 `omd rule 单向/双向 link <tag> <tag2>` 约束 tag 之间的 link 方向；失败级别声明时指定（`--level=warn|fail`，默认 fail）。range commit 之间的 link 允许成环，传播按截断规则自然终止。exclude/include 用可多次传参的 `--exclude/--include`，写法与逻辑参考 .gitignore。
- 查询命令：`omd log <commit-id>` 类似 git log 沿 previous_id 链展示历史（dangling/tombstone 记录也可查看，是手工重放的查看入口）；`omd tree [<commit-id>] [--level project|file]` 类似 bash tree 显示挂载层级，缺省从虚拟全局根开始、不限层级时显示到 range；所有命令支持 `--json`（ED-16 envelope），tree 尤其如此以便脚本集成。tree 只显示挂载层级，不显示 link 引用关系。
- 多用户在同一 OMD 工作产生冲突时，OMD 不提供冲突解决工具（不自动合并、无 mergetool）；commit 支持显式 `--时间戳` 参数（指定值照常参与 hash），用户查看对方历史后手动以指定时间戳重建 commit 序列来解决冲突。
- 多次 unclean 平面列出全部（每条独立、按时间序、全部须处理）；gc 只清理从虚拟根不可达且无任何有效 commit 引用的 commit（被引用的 dangling 保留，commit 清理时其 note 一并清理）。force 关联已取消：remote 不匹配时 verify 报错并提示，用户通过显式更新登记（设置/修正别名）解决，不提供运行时 force 覆盖。
- 坐标、覆盖、持久化、路径发现与 CLI 的工程默认值已逐项过目（ED-01 至 ED-19），详见 design.md 工程默认清单；实现时发现冲突以已确认决定为准。
- 确定 command 验证的执行开关：本次 verify 参数优先于用户配置 `VERIFY_COMMAND_AUTO_RUN`，内置默认值为 `false`。
- 跨项目始终显式关联本地目录，不获取远端内容，不将项目关联固定在内容快照上；可选 Git 来源标识用于 remote 身份核对，并允许用户配置身份对应映射。
- 后续随产品附带的 skill 为 AI 提供可选多仓库工作区方案：在同一父目录下组合 A 项目、外置 OMD 元数据仓库及可选 C 项目各自的 worktree，按用户指定分支配对。只提供使用指导，不增加核心 Git 依赖、跨仓库原子操作或自动 worktree 编排功能。
- 对已确认行为、工程推论和未决问题分别记录，不将候选设计或 CLI 示意冒充完整规格。

## Capabilities

### New Capabilities

下列是后续需要形成规格的能力边界。本次授权只覆盖 proposal 和 design 的增量记录，尚未创建对应 delta specs。

- `managed-content-tracking`: 持续目录纳管、虚拟全局 root 层级、文件初始化/路径迁移 commit、手动 rename 登记、verify 存在性确认、copy 新节点初始化、tombstone 删除、内容版本与范围迁移复核。
- `change-review`: OMD 适配提交、源端分支阻断及显式无理由选项、unclean、状态回退、dangling/unreachable_link/conflict 诊断与回退重建、逐步传播、全量清理验证、commit ID 与平面 note，以及悬空提交的列举、查看和显式重放/清理。
- `command-verification`: verify 中的显式执行覆盖、用户默认值、采集成功条件及快照验证边界。
- `local-project-links`: 本地目录关联、可选 Git remote 身份校验、身份映射及强制关联边界。

### Modified Capabilities

无。当前 `openspec/specs/` 没有既有 capability specs；原有需求基线位于 `docs/`。

## Impact

- 当前产物为本变更的 `proposal.md` 和 `design.md`；不修改 Rust 代码、现有 `docs/`、工作流配置或用户配置。
- 后续实现涉及来源纳管、关系确认记录、CLI、持久化版本依据和身份检查；本次不选择具体依赖、最终 schema 或发布方式。
- 多仓库 worktree 组合为单独的后续 skill 指导任务，见 [design.md](design.md) 的 D-13；本次只记录目标与示例，不创建/安装 skill，不创建 worktree 或分支。
- 追溯原需求 R-04、R-06、R-08～R-11、R-14～R-20，以及 Q-01～Q-05、Q-08～Q-11。关联目录和可选 remote 校验不改变 Git 独立的内容跟踪核心。
- 三态、单写者、旧 hash 冲突即拒绝、可恢复基线、人与 AI 同一规则继续有效；不将 `clean` 引入为第四种状态或自动豁免。
- `commit clean --no--reason` 是用户对 R-08 理由必填要求的明确局部修订；普通 commit 和 commit unclean 的理由要求不因此改变。原 `docs/` 尚未同步，后续合并规格时须同步该例外。
- 现有验收场景仍未执行。本变更尚无 delta specs、实施任务或产品验证结果，不代表实施准备已经完成。端到端生命周期（import → init → range commit → verify 标脏 → 适配 → commit verify → link → 传播/阻断 → reset 级联 → dangling 查看与手工重放 → tombstone → gc → log/tree 查询）已按最终模型在 [design.md](design.md) 纸面走查无矛盾；适配提交与显式阻断实例见 D-07；unclean/reset、无理由 clean、project_id、完整提交输入、commit ID 与 note 见 D-09～D-12；broken 修复、迁移链、层级与生命周期见 D-14～D-19。实施期待定清单集中记录在 design.md，不改变已确认行为。docs/ 原始文档未同步，specs/tasks 阶段须以本目录为准。
