## Context

本文件是探索中的增量契约，不是实施就绪证明。动机与能力范围见 [proposal.md](proposal.md)；原始基线见 [requirements.md](../../../docs/requirements.md)、[open-questions.md](../../../docs/open-questions.md) 和 [acceptance.md](../../../docs/acceptance.md)。本轮未修改原有 `docs/`；后续阅读须同时核对本文件中明确的用户决定。

目前没有 OMD 产品代码或执行过的产品验收。文中的 `commit --reason …`、`commit clean --reason …`、`commit clean --no--reason`、`commit unclean --reason …`、`reset`、`note`、`verify all cleaned` 表达已讨论的产品操作，不是已经可用的命令；对象、范围和分支选择参数尚未冻结。下文简称 clean 时，指 `commit clean` 阻断操作。

本次设计涉及来源、关系、持久记录和验证多个部分，并存在需要在编码前消除的行为歧义，因此需要 design 文档。下文区分：用户已确认决定、由决定直接推出的约束、尚未确认的候选方案。后续每确认一组就在本文件更新，不将未决项悄悄当作默认值。

## Goals / Non-Goals

**Goals:**

- 区分“通过实际修改接续传播”与“带理由阻断传播”，避免要求已经完成适配并提交的节点再补一次 clean。
- 保留源端对 1–n 下游分支的处置权，同时防止一个分支的 clean 抹掉另一个分支或其他变更的责任。
- 明确所确认的是哪个内容版本；目录身份、历史快照和当前观察不能混为一谈。
- 在形成实施规格前，用完整操作实例验证责任能否交接并最终清理。

**Non-Goals:**

- 本轮不实现产品，不创建 delta specs、tasks、Lean 工程、hooks 或运行配置。
- 不引入第四种标记状态、自动豁免、自动语义判断、远端内容获取或固定项目快照功能。
- 不冻结持久化字段、Rust 依赖、锁实现、完整 CLI 文法或尚未讨论的 Q 条目。

## Decisions

### D-01：候选范围迁移后显式复核

**状态：已确认。追溯：R-08、R-09、R-12；Q-01；S-07、S-09、S-21。**

在已确认范围之前插入内容，所选正文没有变化而位置发生变化时，工具给出候选迁移，由使用者显式复核，不能自动迁移后沿用“已确认”。

用户明确允许固定英文理由，例如 `No substantive change.`。理由可由人或 AI 选用，但工具不能仅凭正文匹配自动声称没有实质修改。

选择该方案而非自动保持确认，是为了让迁移与语义判断的责任显式可见。Myers 提供匹配候选，不证明对象身份或语义等价。

**尚未由本决定解决：** 边界插入、范围内修改、删除/拆分、重复文本映射，以及其他位置变化是否使同文件全部标记变脏。不能把本例扩大成“文件 hash 改变必然使全部范围失效”的既定规则。

### D-02：import 目录持续纳管

**状态：已确认。追溯：R-06、R-07、R-09；Q-04；S-03～S-05。**

**范围（用户明确）：** import/remove 的“纳管”仅指统计覆盖率与打 tag 的范围（见本节末尾的 tag 与 rule），不管理文件或 range；文件的跟踪始终靠用户对具体文件的显式 init/commit。因此本条的准确含义是：import 一个目录后，该目录范围进入覆盖率统计分母，后续检查发现范围内的新文件时将其呈现为空及覆盖缺口。不是仅登记 import 时的一次文件清单，也不需要后台监听才能成立；也不等于自动为文件建立跟踪。

未 import 不纳管、允许主动 import `.omd/`、follow 符号链接仍按原需求执行。纳入统计不等于自动确认新文件。

**已由后续决定解决：** 递归边界（全量递归，D-17）、显式排除（exclude/include，D-17）、重叠 import 的对象身份（import 不管理对象身份，D-17）、删除与复制（tombstone / copy 不自动关联，D-16）。文件初始化及重命名的迁移 commit 已由 D-15 补充。

**真正仍待明确的只有一条：** 迁移对已有范围、关联与确认有效性的影响——已由本节末的 D-19 确定。不能把文件重命名检测、自动接受迁移或自动保持确认作为本轮已定行为。reset 不改来源文件的既定边界不变，迁移后的旧路径与实际文件位置也不能靠 reset 静默覆盖。

### D-03：commit 接续传播，commit clean 显式阻断

**状态：已确认，以本轮纠正为准。追溯：R-08、R-09、R-16～R-19；Q-02、Q-08、Q-09、Q-11；S-06、S-07、S-18～S-20、S-22。**

用户明确纠正：“如果 b 适配 commit 了就说明手动 clean 已经被 commit 替代了”，并说明 clean 是 `commit clean --reason …` 子命令，“是阻断行为，而非断言说这里 clean 了，是指到这里为止了”。

| 操作 | 已确认含义 | 对传播责任的作用 |
| --- | --- | --- |
| OMD `commit --reason …` | 记录实际修改及原因；不是 Git commit | 若本次修改适配了指定上游变化，该提交接住该项影响；再处理本次修改对下游的影响，不需要返回上游补 clean |
| `commit clean --reason …` 或 `commit clean --no--reason` | 在当前节点显式阻断所选分支；可以提供理由，也可以明确选择不提供 | 表达“到这里为止”，不是对已完成修改再盖一次确认章；省略理由的例外见 D-10 |
| `commit unclean --reason …` | 显式引入脏状态，不要求内容发生变化 | 增加需处理的状态影响，不因内容 hash 相同被忽略；与回滚的区别见 D-09 |
| `reset <commit-id>` | 将当前有效提交及跟踪状态移到指定 commit 本身 | 保留目标及其之前的有效历史，原先位于目标之后的提交退出当前有效历史；不追加反向提交，不修改源文件，见 D-09 |
| `note` | 对指定 commit ID 添加外挂评论 | 每个 commit 对应一个平面评论数组，不改变该 commit 的传播效果或 ID，见 D-11 |
| `verify all cleaned` | 检查是否仍有未处理的变化/传播责任 | 不要求每个节点或每条关系都具有 clean 调用记录；实际适配提交与显式阻断均可推进完成 |

实际更新并提交，与不继续更新而显式阻断，是处理变化的不同路径。不能将任意不相关 commit 当作已经适配；也不能把必要修改未做且未显式阻断的分支静默当成已完成。阻断可按 D-10 显式省略理由，不意味着可以省略阻断操作。允许选择部分上游变化，见 D-08；具体 CLI 选择参数仍待规格化。

**撤销此前代理的错误推导：** “commit 永远不能代替 clean”“B 已适配并 commit 后，仍须回 A 执行 clean A→B”“每项必须额外 clean 才能通过”。这些不再是有效契约。用户强调不能省略 clean，是不能省略不继续向下修改时的显式阻断操作，而不是所有提交后都必须再 clean；理由可按 D-10 明确省略。

clean 不是第四种标记状态，也不是跳过检查。原有空、脏、已确认三态，以及 skip 不赋予确认资格的要求不变；覆盖如何由关系处理结果聚合仍需 Q-08 细化。普通适配/阻断不能绕过 broken 引用修复：引用悬空提交导致的 broken 属于脏状态，必须先 reset 到依据完整可达的提交，再重新 commit，见 D-14。

### D-04：逐节点接续，由源端选择阻断分支

**状态：已确认，完整实例见 D-07。追溯：R-08～R-10、R-16；Q-02、Q-08、Q-09。**

对 `A --> B --> C`，A 变化后处理 B 是否需要更新；B 实际适配并提交，接住 A 的相应影响，再处理 B 对 C 的影响。每次提交修改后，都可以在当前变更节点执行 `commit clean`，声明无需继续修改其下游；通常提供理由，也可按 D-10 显式省略理由。

- A 的变化无需 B 适配：在 A 端说明理由并阻断相应分支，不要求 B 替 A 解释。
- B 已适配 A 并 commit：不需要额外在 A 端 clean A 到 B。
- B 更新后无需 C 适配：在 B 端执行 `commit clean`，说明原因并阻断到 C。
- A 同时关联 B、C：允许只阻断 A 到 C，保留 A 到 B 的处理责任；也可以显式选择全部出向关联。

```text
    +--> B : adapt and commit
A --+
    +--x C : stop at A, with reason
```

选择源端分支操作，是为了符合一个变更源影响多个下游的模型；不采用目标端逐个替上游说明“不影响”的方式。

**直接约束：**

- 阻断仅对应本次变更及所选关联，不能自动沿用到该节点未来的新变更。
- 阻断 A 的某项影响，不能抹掉 B 自己的改动或其他来源带给 B 的责任。
- 处理 A 到 B 后，不能以推进全局确认依据的方式隐藏未处理的 A 到 C。
- B 适配 A 的提交，不会自动处理 B 的下游责任。
- 已确认的是逐节点处理、实际提交接续和源端阻断，不是“一次无条件将全部可达节点标脏”。

先前提出的“默认传播后收回”与“先裁决再传播”两个时序选项没有被用户选定。不能把其中任一内部时序作为已冻结决定；当前有效操作模型以上述最新澄清为准。

### D-05：verify 来源命令按配置默认值和本次参数执行

**状态：已确认。追溯：R-14、R-15、R-17～R-19；Q-05、Q-11；S-13～S-17、S-20、S-24。**

最初讨论只允许本次显式执行；用户随后补充并修订为：用户配置可以开启默认执行，本次参数可以覆盖。

```text
verify CLI flag > user config > built-in false
```

| 用户配置 | 本次参数 | 是否执行来源命令 |
| --- | --- | --- |
| 未配置或 `VERIFY_COMMAND_AUTO_RUN=false` | 无 | 否 |
| `VERIFY_COMMAND_AUTO_RUN=true` | 无 | 是 |
| 任意 | `verify --run-command` | 是 |
| 任意 | `verify --run-command=false` | 否 |

这里确认的是用户配置键与参数行为，没有额外批准同名环境变量或项目配置覆盖层。

- 未执行时，报告 command 当前内容未验证；不能将历史输出当作本次采集并声称当前内容全部验证通过。这是检查诊断，不是第四种标记状态。
- 正常退出且 exit code 为 0 时，完整 stdout 才能成为本次成功内容；stderr 独立诊断，空 stdout 合法。
- 采集输出未变且原处理依据仍有效时，不因重跑本身要求重新处理；输出改变后不能自动沿用旧提交或阻断结论。输出未变不能抵消另行执行 commit unclean 带来的待处理状态（D-09）。
- 启动失败、非零退出或异常终止时，本次验证失败，保留旧基线，不接受部分 stdout。
- clean 针对已采集的具体版本，不为确认再次运行来源命令。
- 开关只控制 verify 对来源命令的采集，不授权仅因读取配置或重建缓存就运行其中的来源命令。

该选择同时保留默认不执行的边界与用户主动开启自动运行的便利；不采用“永远只能验证旧快照”或“每次都必须手动加许可参数”的限制。

### D-06：跨项目绑定本地目录，Git 标识只作可选身份校验

**状态：已确认；force 的持续语义尚未确认。追溯：R-04、R-06、R-11、R-18、R-20；Q-03、Q-09～Q-11；S-01、S-08、S-12。**

用户明确：“无论如何都只映射本地目录”，并且“跨项目关系一定是固定在目录这个概念，而非某个内容快照”。

- 用户显式关联两个本地目录，内容由用户自行准备；OMD 不负责下载、clone 或更新远端内容。这不是仅限首版的临时取舍。
- 项目 alias 绑定目录，跟踪目录中的当前内容；不提供将项目关联锁定在固定内容快照上的语义。
- 内容 hash、确认依据和必要旧内容属于 OMD 自己的变更/确认历史，不是 Git commit history，也不是项目目录关联的永久内容版本。
- 保留必要历史快照供 Myers 和复核使用，与“项目不固定在快照”并不矛盾；不能因此只存 hash 而丢掉基线。
- 可选的 `git:github.com/anything` 是帮助核对本地仓库身份的信息，不是远端获取指令。
- 配置了该信息后，每次命令运行都核对仓库实际 Git remote 是否仍对应登记身份；不只在首次关联时检查。
- 用户可在配置中显式登记身份对应关系，例如：

```text
git:github.com/anything --> git:git.example/any
```

- remote 不匹配时提示用户，不自动接受或静默修改绑定。**force 关联已由 D-21 取消**：用户通过显式更新登记（设置/修正别名）解决，不提供运行时 force 覆盖。

只关联目录而非固定快照，是为了持续跟踪演进中的项目。remote 检查是显式配置后的身份约束，不让无该配置的核心内容跟踪依赖 Git、Git 历史或联网。

**尚未由本决定解决：** 校验哪个 remote、不同 URL 形式的比较规则、身份映射结构、校验不可用时的处理，以及 force 是一次性许可还是变更登记身份。特别不能把 force 默认为永久关闭身份检查。

### D-07：实际适配提交接住上游影响，不补重复 clean

**状态：按本轮用户纠正形成的行为实例；未执行产品测试。追溯：D-03、D-04；Q-02。**

以下只涉及给出的关联，假设不存在其他内容变化、缺失标记或检查失败。箭头表示影响处理方向，不代表最终关系类型名称；表中的分支选择说明不是已经冻结的 CLI 参数。

```text
A --> B --> D
|
+--> C
```

| 步骤 | 操作 | 尚未处理的传播责任 |
| --- | --- | --- |
| 1 | A 修改并执行 `commit --reason …` | A 对 B、C 的影响 |
| 2 | 在 A 端执行 `commit clean --reason …`，选定到 C 的分支，说明 C 无需适配 | A 对 B 的影响；到 C 在 A 处阻断 |
| 3 | B 为适配 A 而修改并执行 `commit --reason …` | B 对 D 的影响；B 的适配提交已接住 A，不需要回 A 补 clean |
| 4 | 在 B 端执行 `commit clean --reason …`，选定到 D 的分支，说明 D 无需适配 | 无；到 D 在 B 处阻断 |
| 5 | 执行全量清理验证 | 本例所列责任已经处理；其他检查同样满足时才能整体通过 |

若 D 确实需要适配，步骤 4 应改为实际修改 D 并提交，然后继续处理 D 的下游，而不是由 B 以 clean 声称“D 已经修改完成”。这两种操作分别表达接续与阻断，不能再要求两者依次发生才算完成。

实现必须能够辨别一次适配提交处理了哪些上游变化，以及一次阻断针对哪些出向分支，否则会误消除无关责任。部分上游选择已由 D-08 确认；具体选择参数、版本依据字段、聚合方式及持久布局尚未冻结，这不是已经批准的具体 schema。

### D-08：适配 commit 可显式选择部分上游变化

**状态：已确认。追溯：R-08、R-09、R-18；Q-02、Q-09、Q-11；D-03、D-04。**

A、X 同时影响 B 时，允许 B 的一次适配提交只处理明确选定的 A 变化，保留 X 的待处理影响。可选择一个、多个或全部上游变化；不要求全部上游都完成适配后才能提交。

```text
A --+
    +--> B --> C
X --+
```

| 操作 | 结果 |
| --- | --- |
| B 适配 A，并在 commit 中明确选择 A 的这次变化 | A 到 B 的对应影响已处理；X 到 B 仍待处理 |
| B 的本次修改影响 C | 接着处理这次 B 到 C 的影响 |
| 在 B 执行 commit clean，阻断这次变化到 C | 只处理该出向分支，不能消除 X 到 B 的未处理影响 |
| 执行 verify all cleaned | 只要 X 到 B 仍未处理，就不能因 A 分支已处理或 B 已阻断到 C 而通过 |

选择此方案是为了支持分步适配，并保留每项变化的责任；拒绝“任意 B 提交都清空 B 的全部上游影响”和“必须全部适配才能提交”两种行为。

**沿用已有版本约束：** 提交依据使用者先前读取的具体版本。所选上游内容或本次提交依据的相关内容/记录在写入前发生版本冲突时，按 R-18 拒绝，不改用新 hash 静默重试。command 以已采集的观察版本为依据，不为此重新运行。

**尚未由本决定解决：** 上游变化的标识与选择参数、expected hash 的精确粒度，以及多次未处理变化的展示/归并方式。不能仅凭理由文本猜测处理了哪些来源，也不能把来源选择等同于语义适配已经被核心证明。

### D-09：commit unclean 叠加脏状态，reset 移动有效历史位置

**状态：操作区别、reset 名称、目标提交本身保留及源文件不改已确认；回退导致悬空引用时的 broken 判定和修复已由 D-14 明确。reset 可作用于除虚拟全局 root 外的任意 commit，上层 reset 级联到子节点（D-16）；长期保留/清理细节仍待定。追溯：R-08、R-09、R-18、R-20；Q-02、Q-09、Q-11。**

用户先明确状态操作区别：“revert 不是叠加是回滚，而 commit 可以直接 commit unclean 状态，这样无论有没有修改都必须处理脏状态，也就是所谓的层叠。”随后修订命名与语义：“revert 叫做 reset，逻辑跟 git 一模一样”。以最新修订为准。

- 支持显式 `commit unclean`：即使内容没有改变，也能引入必须处理的脏状态；理由要求沿用已有提交契约。
- unclean 是一次新的状态提交，不等于取消以前的提交，也不能只因当前内容 hash 与之前相同就当作无操作。
- `reset c1` 将所选回退作用域的有效状态移到 c1 本身，保留 c1 的效果，该作用域内 c1 之后的提交退出当前有效状态。用户已确认 reset 可作用于除虚拟全局 root 外的任意 commit，影响是使其后的 commit 悬空；上层 commit reset 级联到所挂载的文件节点（D-16）。
- 历史记录保留与当前关联有效性分开：A reset 到 a0 后，a1 数据仍在但从当前有效根不可达；B 的 b1 仍引用 a1 时，verify 报 B 为 broken（脏），不自动改写引用或回退 B。修复由使用者将 B reset 到依据完整可达的提交后重新 commit，见 D-14。
- **取代旧表述：** 不再将操作称为 revert，也不再将 `reset c1` 解释成“退回 c1 之前”。要撤下 c1，应选择 c1 之前的提交作为目标；reset 目标语法已由 D-19 确定为仅完整 commit ID 或唯一前缀。
- 继续遵守此前明确选择的状态边界：只操作 OMD 的提交与跟踪状态，不恢复或改写用户源码、文档等来源内容，不运行来源命令补偿过去的副作用。
- 不追加反向 commit，不要求从末尾逐条撤销。reset 本身改变当前有效历史位置，不物理删除原 commit 及其外挂 note；记录未被另行清理时，仍可按原 ID 查看。退出有效历史与删除历史是两件事。
- reset 后仍检查真实来源内容。若磁盘内容与目标状态的基线不同，该差异仍须处理；移动历史位置不意味着当前内容自动干净。
- `commit clean` 仍表达源端阻断，不被本次补充改成完成盖章。unclean 也不增加第四种标记状态。
- 这是 OMD 自身的操作，不调用 Git 来保存或回退核心状态。Git 的目标提交语义不意味着在 OMD 中新增 Git index、分支系统、hard reset 或来源文件覆盖能力。

**一手资料校准：** [git-reset](https://git-scm.com/docs/git-reset) 的提交形式把 HEAD 指向目标 commit 本身；默认 mixed 与 soft 都不改工作树，hard 则会改。本项目已明确不改来源内容，不直接套用 hard 模式。[git-notes](https://git-scm.com/docs/git-notes) 将注释与被注释对象分开；[git-gc](https://git-scm.com/docs/git-gc) 说明不可达对象的清理另有保留与回收规则，不能把 reset 当成立即删除，也不能由此承诺永久保留。

**直接工程约束：** 内容版本与跟踪状态版本必须能区分。相同内容 hash 可以具有不同的未处理影响；显式 unclean、阻断和 reset 都会影响跟踪结果，不能把内容 hash 单独用作完整状态身份或索引有效性的依据。commit ID 的盐位于 hash 输入第一字段，具体元数据版本编码和 expected hash 粒度仍属 Q-09。

**尚未冻结：**

- 如何查找所有必要依据均可从根索引的回退目标的具体算法（可达性定义已由 D-16 给出）。D-14 的悬空引用判定已经确定，不得继续将其视为待决，也不能把派生的 broken 诊断等同于改写 B 的历史提交。
- 多次 unclean 的选择、合并展示方式。用户确认需要层叠，每个提交按 D-11 的随机盐/前驱/时间戳/本地内容公式生成独立 ID，但尚未冻结具体计数或存储模型。

**已由后续决定解决：** 退出当前有效历史的 commit、note 及必要基线的长期保留/清理策略——由 D-19 的显式 `omd gc` 命令解决。

### D-10：commit clean 可显式省略理由

**状态：已确认，修订此前理由必填要求的局部边界。追溯：R-08、R-18、R-19；Q-02、Q-09、Q-11；D-03、D-04。**

用户明确要求 `commit clean --no--reason` 可以不回答原因，例如前一个 commit 的理由只是对以前状态的补充说明和修订，不需要为了阻断再编造一段理由。

- 按用户给出的拼写记录 `--no--reason`；本轮不擅自更名为 `--no-reason`。
- 不选择该开关时，仍按既有 clean 契约提供理由；选择该开关时，可以不提供理由，仍产生有效的显式阻断操作。
- 工具需要区分“明确选择不提供理由”和“遗漏必填输入”，不能替用户伪造固定理由，或自动把上一条说明复制成新的阻断理由。
- 该开关只改变理由要求，不扩大所选分支、不跳过版本校验，也不改变 clean 的源端阻断语义。它不是 skip，不是自动识别“仅说明修订”的语义规则。
- 本次例外限于 `commit clean`；不据此把普通 commit 或 commit unclean 的理由默认改为可省略。

此前 R-08 和本次设计中无例外的“更新均需理由”表述，在此被用户明确细化；原 `docs/` 尚未同步，后续合并规格时应同步该例外，不能拿旧基线否定本轮决定。

### D-11：随机盐、前驱、时间戳与本地内容派生 commit ID；note 外挂

**状态：盐优先、完整提交输入原则，以及 project_id 仅作项目元数据而不进入 commit hash 已确认；具体字段 schema、前驱所属范围、字节编码和 reset 作用域仍需规范化。追溯：R-08、R-18～R-20；Q-02、Q-09、Q-11；D-03、D-09、D-10。**

用户核对 Git 对象身份后，明确选择“移出 hash，保留元数据”：`project_id` 继续用于项目登记和解析，不额外把项目归属绑定到提交 ID。该决定取代此前将 project_id 放在盐之后参与 hash 的方案。16 位随机盐仍是第一字段；本次提交的内容和完整不可变操作输入仍参与 SHA-256。区块链只作为前驱哈希 ID 的设计参考，不代表 OMD 采用区块链产品架构。

当前输入类别和顺序为：

```text
salt_16_chars = 16-character random string
previous_id = this node's previous commit ID, or "" if absent
timestamp = commit timestamp
content = local content under review
operation_payload = immutable operation inputs
commit_id = SHA256(salt_16_chars + previous_id + timestamp + content + operation_payload)
```

上述 `+` 表示按此字段顺序组成输入；盐是第一字段。`content` 覆盖本次提交涉及的本地内容，`operation_payload` 覆盖完整不可变操作输入，例如操作类型、理由状态/内容、选定上游 commit、来源/范围和 expected 版本。项目登记的归属信息不作为额外输入，也不通过 operation_payload 重新加入 project_id。note、缓存、当前派生状态和提交之后产生的数据同样排除。具体字段 schema、前驱所属范围以及 canonical 编码仍需规范化，不能把这段示意直接当作最终 schema。

- 盐固定在输入最开始；每次新 commit 生成新的 16 字符随机盐，不重新使用已有提交的盐和时间戳来冒充一次新提交。
- `project_id` 是权威元数据中的稳定逻辑项目身份，首次显式 import 或初始化时由 OMD 生成并持久化，不由本机路径、当前 Git remote 或目录内容自动推导。项目复制、路径变化或 remote 变化不自动改变它；显式身份迁移记录旧新身份关系，但不会重算已有 commit ID。
- 复制或引用已有提交时，保留其原始输入和 ID，不生成新盐、不把目标目录的项目身份注入旧记录。新发生的提交才生成新盐和时间戳。提交身份保持与项目登记归属是两个问题。
- 前驱是当前提交所属逻辑节点的上一条 commit，不是整个元数据目录最近的任意 commit，也不是所适配的其他节点的上游 commit。首条提交使用空字符串，不擅自替换为 null、全零 hash 或虚构父提交。“逻辑节点”的归属已由 D-16/D-17 确定为挂载树上的节点（文件节点、range commit 链、import 分支链等），不再是开放问题。
- 已确认 ID 绑定本次提交的完整内容和操作输入：盐、前驱、时间戳、本地内容和不可变操作元数据参与 SHA-256；项目登记信息不属于这些操作输入。具体字段 schema、字段顺序和 canonical 编码须由后续 schema 统一规定。
- 普通内容提交、`commit clean`（包括 `--no--reason`）、`commit unclean`，以及 D-15 的初始化/迁移 commit 应使用同一 ID 构成规则。即使本地内容相同，也不能把不同的状态提交合并成一条；每次新提交生成新的盐并检查已知 ID 是否冲突，碰撞时重新生成盐计算，不能覆盖旧记录。
- reset 不重写历史 ID；其后的新提交重新按最终公式计算，不复用已退出当前有效状态的旧提交 ID。reset 作用域已由 D-16 明确为“除虚拟全局 root 外任意 commit，级联到子节点”。
- 为使 ID 可复算，保留本次使用的盐、前驱、时间戳、内容依据和完整操作输入；复算时不重新取时间、随机盐或当前变化后的内容，也不查询当前项目身份来补充 hash 输入。具体持久布局仍属 Q-09。
- note、缓存、当前派生状态和提交之后产生的数据不属于原 commit ID 的输入；外挂 note 不改变原 commit ID。

**已由 ED 清单收口的输入细节：** 完整操作输入的字段 schema、`content` 的精确范围、前驱所属范围、canonical 字节编码、时间戳表示、盐字符集及 ID 显示格式均见 ED-01 至 ED-19。project_id 的登记与迁移是独立元数据契约，不重新引入 commit hash。

**参考与边界：** [Pro Git：Git 对象](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects) 描述按对象头与内容生成对象 ID，不包含额外的仓库身份；同一对象复制到其他仓库仍保持 ID。OMD 借鉴这种身份与存放位置分离，不照搬 Git 的 hash 算法、对象格式或整个架构。

**需与 D-09 联合核对：** project_id 的登记不自动决定逻辑节点、前驱范围或 reset 作用域。跨项目关系是传播/关联边，关联不等于合并提交历史。reset 的目标提交保留、来源文件不改的约定不变。

`note` 是独立子命令，对指定 commit ID 添加评论。一个 commit 可以有多个 note，采用一维、有序的平面数组；不引入父评论、嵌套回复树或 PR 审批流程。

**note 的存储形态（用户明确）：** note 是单独的数据存储地方，只是一个对应 note 内容、note 时间和 commit ID 的记录。它可以对任意 commit 进行 note；任意 commit 也可以反查自己的 note list。note 不是 commit，不进入任何前驱链。

```text
commit c1 --> notes [n1, n2, n3]
commit c2 --> notes []
commit c3 --> notes [n4]
```

上图只表达关联形状，不是文件布局或最终字段定义；n1 等仅为示例标签，不是在此决定 note 的 ID 格式。

**外挂评论的直接约束：**

- 增加 note 不改被评论 commit 的 ID，不隐式改写其原始操作或理由，不自动执行 clean、unclean 或适配提交。评论可以补充/纠正说明，但不会仅因其文本就改变传播状态。
- 用户选择创建 clean commit，即使使用 `--no--reason`，仍然是有 ID 的状态操作；单独发一条 note 则只是评论。不能把两条路径混为一谈。
- note 是持久的用户信息，不能只存在于可删除的 SQLite 缓存；写入继续遵守单写者和旧版本冲突拒绝要求。

**尚未决定：** note 作者字段。note 的时间字段已确认必须存在。编辑/删除与长期保留已由 D-19 解决（只增不改不删，修改/删除作为新 note 叠加，gc 显式清理）。reset 本身不删除 commit 或 note，见 D-09。不额外添加回复树、评论解决状态、权限或通知系统。

### D-12：Git 样式状态数据库，区块链仅作 ID 生成参考

**状态：产品边界已确认；完整提交输入的具体字段 schema、逻辑节点身份、reset 作用域和持久化细节待定。追溯：R-01、R-04、R-08、R-18～R-20；Q-02、Q-09、Q-11。**

OMD 的目标是一个带提交、当前有效状态、关系传播、note 和 reset 的 **Git 样式状态数据库**。它不以 Git 为核心依赖，也不是 Git 的替代品；它管理的是文档—代码—图之间的关系处理状态。

区块链只提供 commit ID 的一个设计参考：使用前驱 commit ID、时间戳、盐和提交输入参与 SHA-256，使提交身份具备可追踪的前驱关系。这个参考不引入区块链的产品特性。

明确排除：

- 不引入不可验证篡改证明作为产品目标。
- 不引入 block/head 作为独立产品架构术语或额外生命周期。
- 不引入 PoW、挖矿、网络广播、节点共识、分布式账本或 Git index。
- 不把跨项目关联自动拼成一条全局提交链。
- 不从 ID 公式额外推导第四种状态、自动语义证明或历史不可删除承诺。

提交状态、传播关系、note、来源快照和当前有效记录仍按 OMD 自己的权威文本与可重建索引模型设计。`project_id` 保留为项目登记和解析元数据，不进入 commit ID；它不决定全部关系传播，也不替代 alias、来源、范围或版本记录。

**已由 ED 清单和 D-16 收口：** 逻辑节点身份、前驱所属范围、内容与提交载荷字段、canonical 编码、reset 对有效状态的影响、权威文本布局均见 ED-01 至 ED-19 及 D-16。历史记录保留策略见下文 note 段。不能因为 ID 参考区块链，就自动选择链、block、head 或不可变存储架构。

### D-13：未来产品 skill 提供 worktree 目录组织方案

**状态：后续附带 skill 的任务已确认；不是新增核心能力或本轮 skill 实现。追溯：R-01、R-03、R-04、R-11、R-19、R-21；Q-12。**

用户要求把 Git worktree 作为未来 OMD 随附 skill 提供给 AI 的现成使用方案：引导用户舒适地组织外置 OMD 元数据仓库、多仓库本地 link 和对应工作目录，避免 AI 或用户重新发现/发明已有方案。

用户给出的具体组合是：一组工作区放 A 项目的 B 分支、OMD 元数据仓库的对应分支、可选 C 项目的 A 分支；另一组放 A 项目的 main 分支与其余仓库的对应分支。不同仓库的分支名不需要相同，“对应”由用户选择，不由工具根据名称猜测。

以下仅是未来 skill 的目录示意，不是已创建目录或可执行配置：

```text
workspaces/
+-- feature-b/
|   +-- project-a/  [repo A, branch B]
|   +-- omd-data/   [repo OMD, matching branch]
|   +-- project-c/  [repo C, branch A; optional]
+-- main/
    +-- project-a/  [repo A, branch main]
    +-- omd-data/   [repo OMD, matching branch]
    +-- project-c/  [repo C, matching branch; optional]
```

父目录提供组合工作区；每个子目录分别属于自己的 Git 仓库，可以是该仓库的 linked worktree。不是让一个 worktree 同时属于 A、OMD、C 三个仓库，也不是通过父目录创建新的统一仓库。

- skill 在用户采用 Git、确实需要多个工作目录时推荐该方案，并提供经过验证的目录布局、外置元数据位置和本地 alias 配置示例；不强制迁移用户已有布局。每组使用自己配对的 OMD 元数据目录与来源目录，避免映射误指向另一组的同名项目。
- C 是可选成员；只有 A 与外置 OMD 元数据仓库也可组成工作区，不要求为了示例补造第三个项目。
- 组合类似 workspace 的目录体验，但不是 Cargo workspace 那样由工具原生统一管理的成员集合。Git worktree 不提供跨仓库原子的 checkout、commit、reset、merge 或 push；一组目录也不代表三份仓库内容已经语义同步。指南不承诺自动配对、同步分支或新增工作区编排器。
- worktree 是外围目录组织工具，不成为 OMD 的来源、项目身份、差异、基线、版本或锁实现依赖。普通目录使用不变。
- [Git 官方 worktree 文档](https://git-scm.com/docs/git-worktree) 明确 linked worktree 属于同一个 Git 仓库，不会把多个独立仓库自动合并成一个工作台。指南应说明：不同仓库各自准备 worktree，再通过父目录布局和 OMD 显式本地关联组合起来。
- 外置 OMD 元数据仓库也可使用自己的 worktree。Git 的工作目录隔离不等于 OMD 并发写入许可；多个调用若指向同一元数据根，仍遵守单写者约束。
- 目录移动、worktree 移除与 alias 路径维护需要在后续指南中交代，不承诺 Git 自动修复 OMD 的本地映射。不把 Git refs 的共享或默认同分支检出限制隐藏起来，也不默认建议 force 绕过。

本轮只记录附带任务，不创建/安装产品 skill，不执行 git worktree、分支或仓库创建命令，不新增 OMD 自动编排 worktree 的功能。具体指南和示例在产品 CLI/映射契约明确后单独编写与验证。

### D-14：悬空引用导致 broken，reset 后重建；支持悬空提交管理

**状态：判定、修复路径、诊断术语与功能目标已确认；根索引规则、重放与清理细节待定。追溯：R-09、R-18、R-20；Q-02、Q-09、Q-11；D-03、D-09、D-11。**

用户明确：A reset 到 a0 后，不直接删除 a1。B 的 b1 指向从根树搜索不到的悬空 a1，verify 应警告 B 的状态是 broken，且 broken 是脏状态的一种。修复只能先 reset 到如 b0 这样所有必要信息均可从根索引的提交，再重新 commit。还需提供查找磁盘上仍存在但已悬空的 commit，以便查看、重放或清理其数据。

**诊断术语（用户明确）：** 悬空本身叫做 **dangling**（悬置）；因 dangling 关系而 broken、需要修复的状态叫做 **unreachable_link**（断链）。unreachable_link 与 dangling 都是 dirty 的诊断原因，不是第四种标记生命周期。

#### 判定与修复

- **磁盘存在不等于当前有效可达。** a1 即使仍能按 ID 读取，也可能已退出当前有效根所索引的历史。b1 对 a1 的引用不能仅凭“还能读到数据”就使该适配有效，也不能反过来把 a1 自动纳回有效历史。
- **broken 是脏状态的诊断原因。** 对受影响的 B 报告 broken 及其悬空引用，不新增第四种标记生命周期。有未修复的 broken 时不能声称 `verify all cleaned` 已通过；显式跳过检查也不修复它。
- **必要依据的间接断裂同样导致 broken。** 用户补充确认：若 c1 依赖 b1，b1 本身仍可索引，但其所依赖的 a1 已悬空，则 verify 此时也报告 C 为 broken，不必等 B reset。诊断展示如 `c1 --> b1 --> a1 (dangling)` 的原因路径。这是引用依据完整性检查，不改变 D-04 普通内容变化逐节点处理的规则，也不自动回退或改写 B、C。
- **必须先回退到完整可用的依据。** 为 B 选择如 b0 的提交，其直接和间接必要依据均完整可用、可从当前有效根索引，然后 reset B，再基于当前有效依据重建提交。候选回退目标也不能仅凭直接引用可找到就判定可用。不能在 broken 的 b1 上追加普通 commit、commit clean（包括无理由模式）、commit unclean 或 note 来跳过这个修复步骤。
- **不改写旧记录。** 不把旧 b1 中的 a1 静默替换为 a0，不因 b1 broken 自动删除它或其评论。重建是新的提交，按现有盐/时间戳/前驱规则生成新 ID，不能冒用原 b1 ID。
- **诊断提供可执行的修复线索。** 指明受影响节点、当前 unreachable_link commit ID、dangling 目标 ID，以及经过检查的可回退 commit ID；例如说明需 reset 到 b0 再重新 commit。找不到合格目标时如实报告，不能编造 b0 或推荐同样含悬空依据的记录。精确 CLI 参数另行规格化。
- reset 仍不修改来源文件。来源实际内容与恢复后的基线是否匹配，由已有内容检查规则单独判断；引用完整并不自动证明内容或语义正确。

**项目根与挂载树（用户补充确认）：** 同一项目中的 A、B 等文件节点都指向一个项目根；这个根只用于挂载这些文件节点。这是项目结构可以构成树的前提。这里描述的是 OMD 的逻辑挂载关系，不是要求磁盘目录改成某种布局。

挂载关系与提交的适配/必要依据引用须区分。b1 对 a1 的引用用于完整性检查，不能因诊断时能沿此引用读到 a1，就把已退出有效历史的 a1 重新算入根下的有效记录。项目根的挂载职责已经确定；文件节点的当前提交与有效历史的遍历规则已由 D-16 的可达性定义（虚拟根 → add project → import → 文件节点）确定，不再是开放问题。D-15 进一步明确 B0 是文件节点的初始化 commit，不是将项目根本身当作 commit，也不需要据此另造“无提交的回退目标”；初始化只登记文件自身，不与上游适配合并。

#### 悬空提交管理

- 提供列出指定 OMD 存储检查范围内所有“磁盘仍存在、但从当前有效根不可达”的 commit 的功能，不只列出本次 reset 刚撤下的记录。CLI 入口已由 D-17 确定为 `omd list --dangling`。
- 支持按 ID 查阅这些提交的数据；查看本身不使其重新有效，不创建新提交，也不触发来源 command。
- 支持用户显式查看或清理悬空提交。**重放的含义（用户明确）：** 所谓“重放”不是 OMD 的自动操作，而是用户用 CLI 命令打开该 commit 查看信息，然后自己手工在别的地方重新应用。OMD 不提供“一键重放为新提交”或“恢复旧提交有效性”的命令。清理策略已由 D-19 确定为显式 gc 命令，由用户主动触发。
- reset 与 verify 不自动执行重放或清理。不能因为诊断指出 a1 悬空，就擅自恢复 a1 以掩盖 B 的 broken，或立即删除 a1。批量清理应有明确目标和写入前的版本校验，不能让其他有效数据丢失。

### D-15：初始化与文件路径迁移 commit

**状态：初始化/迁移 commit 及其 path.source、path.target 含义已确认；初始化只登记文件自身，不与上游适配合并。迁移执行与关联处理细节待定。追溯：R-06、R-08、R-09、R-18、R-20；Q-04、Q-09、Q-11；D-11、D-14。**

用户明确：文件重命名如 B 到 C、随后 A 到 B，应记录为迁移 commit；迁移的 path 中有 source 和 target。文件节点的第一条 B0 是初始化 commit，没有 source，或省略 source，只有 target。文件节点挂载于项目根，根的职责见 D-14。

以下只展示 path 字段，不是完整提交 schema 或可执行命令：

```text
B0 initialization: path = { target: "B" }
B-to-C migration:  path = { source: "B", target: "C" }
A-to-B migration:  path = { source: "A", target: "B" }
```

- 初始化的 path.source 省略，path.target 指向初始路径；不能用项目根路径冒充迁移 source。迁移的 path.source 是迁移前路径，path.target 是迁移后路径。
- **由示例直接推出：** B 到 C 后，A 可以迁移到已经腾出的 B。此时路径 B 对应原 A 的迁移结果，原 B 已迁到 C；不能因路径字符串再次为 B，就把原 B 的提交、note 或关联悄悄接到原 A 上。具体节点标识格式仍未选定。
- **沿用 D-11：** 初始化/迁移都是有 ID 的 commit；操作类型与实际 path 输入属于不可变操作输入，参与 commit hash。首条初始化 commit 的 previous_id 按既有规则为 `""`，它与省略 path.source 是不同字段。项目根的挂载关系不因此变成一个虚构的前驱 commit。
- **术语边界：** path.source/target 表示文件位置；previous_id 表示提交前驱；上游适配依据是另一类引用。只有 target 的路径记录，不足以决定初始化 commit 是否同时允许记录上游适配依据。初始化也不自动赋予内容“已确认”或覆盖资格。
- **初始化与适配分开（用户明确确认）：** B0 初始化只登记该文件节点的初始化，不能同时记录“已经适配上游 a1”；上游适配只能在初始化之后的适配 commit 中记录。这样 a1 悬空时，B0 自身不因一项上游适配依赖而 broken；初始化中的该文件能否成为合法回退目标，仍须检查其内容与后续迁移/其他必要依据是否完整。

**初始化时机（用户确认）：** 用户明确“import 是 import 文件夹的 commit，init 是 commit 文件的 commit”，并确认推论：init commit 由用户对文件的显式 commit 动作产生，import 不批量生成 init。未 init 的文件在 verify 中呈现为未标记缺口（D-02）。

**仍待明确：** 迁移对已有范围、关联与确认有效性的影响——已由 D-19 确定（迁移后保持确认与关联，仅 path 变化）。

## Contract Walkthrough

## Contract Walkthrough

本节组合已确认操作与 R-18 的约束，不新增状态或操作。它是纸面推演，不是产品测试通过记录。示例节点代表已经选定的内容范围，关系和范围在本例中不增删、不发生定位歧义；范围之外的内容管理不由本例决定。

### D-16：虚拟全局 root、手动 rename commit、verify 存在性确认、copy 不自动关联、tombstone 删除

**状态：虚拟全局 root、手动 rename commit、verify 存在性确认、copy 不自动关联、tombstone 删除均已确认。追溯：R-06、R-09、R-18、R-20；Q-04、Q-09、Q-11；D-09、D-14、D-15。**

用户明确：

- **虚拟全局 root（唯一根）：** 项目根再往上还能绑定一个虚拟全局 root，用于关联所有本 OMD 关联的项目（如 git 仓库）。file path 的稳定基座是虚拟全局 root → 项目根 → 文件节点。项目根的加入通过 **add project commit** 记录。
- **commit 的层级对象（用户明确）：** import commit 是 import 文件夹的 commit；init commit 是 commit 文件的 commit。两者分别作用于目录层级和文件层级，不是同一个 commit 的两个阶段。
- **手动 rename commit：** 迁移 commit 由用户手动 commit 产生，不是 verify 自动检测。verify 只确认最新的所有 commit 所指向的文件是否真实存在于磁盘上（逐个算 hash 确认）；它不主动发现改名、不自动登记迁移。
- **copy 不自动关联：** 复制产生新节点，目标路径走初始化 commit；不自动记录来源、不自动建立传播关系。如需跟踪，另行显式 link。
- **不主动管理未纳管文件：** 不在 import 目录下的文件不受管理，verify 不检查它们。
- **tombstone 删除：** 删除是一种 commit：节点有效状态记为已删除，历史和 note 保留可查；仍引用它的依赖方报 broken。磁盘上文件消失但没有 tombstone 时，verify 报节点缺失（脏），区分有意删除与文件丢失。

#### 层级模型

```text
virtual_global_root (唯一根)
  +-- project_root A (add project commit)
  |     +-- file node B: B0(init) --> B1 --> ...
  |     +-- file node C: C0(init) --> C1 --> ...
  +-- project_root X (add project commit)
        +-- file node Y: Y0(init) --> Y1 --> ...

add project commit: 将 project_root 挂到 virtual_global_root 下
import commit:      文件夹层级的范围 commit（全量，仅用于统计覆盖与打 tag），可由后续 exclude/include commit 层叠修改；不管理文件或 range
file init commit:   文件层级的初始化 commit（只有 path.target）
range node:         文件节点下的字符/字节 range 跟踪节点，是真正会变脏的对象
command object:     与文件节点平行的虚拟文件对象，由 commit 初始化，比对的是运行输出而非磁盘 hash
```

**每个 commit 都记录时间（用户明确）：** 无论哪一层、哪一种 commit，都必须记录提交时间。这与 D-11 中 timestamp 参与 hash 一致。

#### 树的第四层：range 节点与文件 commit 的职责（用户明确）

树不止到文件层面，而是到字符 range 层面：一个文件的 init commit 节点下再出现很多字符/bytes 跟踪节点。**这些 range 节点才是会变脏的对象；文件节点 commit 本身不会变脏。**

文件 commit 节点存在的意义是跟踪 hash 值的变化：

1. 文件 hash 变化后，verify 用 Myers 算法（即 git diff 的算法）检查该文件节点下哪些 range 节点受影响，**只有算法指出的受影响 range 才标记为脏**，未受影响的 range 保持已确认。
2. 用户逐个解决脏 range 并 commit。
3. 当该文件下所有脏 range 都解决后，用户运行 `commit verify <文件路径>`（参数形式类似 git add）。若 verify 通过，产生一个 **hash 变化的 commit**，记录该文件最新的 hash。

```text
file node B: B0(init, hash=h0) --> B1(hash changed, hash=h1) --> ...
  +-- range r1: r1.0(confirmed) --> r1.1(dirty, Myers hit) --> r1.2(confirmed, user commit)
  +-- range r2: r2.0(confirmed)   [Myers 未命中，保持已确认]
  +-- range r3: r3.0(confirmed) --> r3.1(dirty) --> r3.2(confirmed)
→ r1/r3 都解决后，`commit verify B` 通过，产生 B1(hash=h1)
```

这与 D-01 “候选迁移+显式复核”兼容：Myers 给出哪些 range 受影响，用户逐个复核；不因文件 hash 变化就把全部 range 标脏。

#### command object（用户明确）

注册的 command 在项目中本质是一种**虚拟的文件对象**，只是查的不是 hash 而是运行命令。它同样需要 commit 命令去 init 一个 command object。如果运行结果与之前不一样，它同样变成脏对象。

这与 D-05 “默认不执行、显式覆盖”兼容：是否在 verify 时运行命令仍按 D-05 的配置/参数决定；command object 的“内容”是上次采集的 stdout，“hash”是这段 stdout 的 hash。其下同样可有 range 节点。命令未运行时不判脏，也不判洁。

上图中各层 commit 的具体归属关系（例如 import commit 是否直接挂在 project_root 下）尚未完全冻结；已确认的是三种 commit 各自对应的对象层级。

挂载关系与提交的适配/必要依据引用须区分。verify 的存在性确认只检查最新 commit 指向的磁盘文件是否存在且 hash 匹配；它不沿适配引用恢复悬空记录，也不把 dangling 记录重新算作有效。

#### rename commit 的手动流程

1. 用户在磁盘上把 B 改名为 C。
2. 用户显式执行 rename commit，记录 `path = { source: "B", target: "C" }` 和 reason。
3. verify 检查最新 commit（C 的 rename commit）指向的 C 是否在磁盘上存在且 hash 匹配。
4. 若用户未登记 rename 而直接改了磁盘，verify 发现最新 commit 指向的 B 不存在，报节点缺失（脏）；C 作为新文件呈现为空（未标记）。用户需补登记 rename commit 才能恢复跟踪。

#### copy 与 delete 的行为

- **copy：** B 复制出 C 时，C 走自己的 B0 初始化 commit（`path = { target: "C" }`），与原 B 无自动关联。原 B 的历史、note、链接不受影响。
- **delete：** 删除通过 tombstone commit 记录。用户明确：tombstone 本质是迁移 commit 的一种——`path.source` 为原路径，`path.target` 指向 null。它不是独立的 commit 类型。节点的有效状态变为「已删除」，历史和 note 保留可查。仍引用该节点的依赖方报 broken（具体诊断名后续归类）。磁盘上文件消失但没有 tombstone 时，verify 报节点缺失（脏），与 dangling 区分：dangling 是记录仍在但退出有效历史；缺失是磁盘文件不在。

```text
B0 initialization:  path = { target: "B" }
B-to-C migration:   path = { source: "B", target: "C" }
C tombstone:        path = { source: "C", target: null }
```

由此直接推出：tombstone 后同路径再出现文件，走新的初始化 commit（`path = { target: "C" }`），是新节点；旧链以 `target: null` 终结，不与新链自动接续。

#### reset 的作用范围（用户明确）

用户明确：reset 可以作用于**任意 commit**，唯一例外是虚拟全局 root 本身不能被 reset。影响是使目标之后的 commit 悬空（dangling）。这取代 D-09 中“一般回退作用域仍待定”的表述。不修改磁盘、不删除记录的约束不变。

**级联到子节点（用户确认）：** 对上层 commit（如 add project、import）reset 时，其下挂载的文件节点提交一并退出有效历史。符合“从虚拟全局 root 可达才有效”的树模型：父退出，子自然不可达。D-14 的可达性定义因此为：从虚拟全局 root 沿当前有效的 add project → import/exclude/include → 文件节点链能到达的 commit 才是有效的；任一层退出，其下全部 dangling。仍引用这些 dangling 记录的其他项目节点按 D-14 报 unreachable_link。

#### import 的范围与层叠（用户明确）

import commit 是全量将该文件夹（含子目录）计入覆盖率统计范围。之后用户可用 **exclude commit** 和 **include commit** 层叠调整范围，逻辑是层叠：后的 commit 覆盖前的同范围判定。不添加隐式排除；`.omd/` 仍可显式 import。

**语法（用户明确，先定下来，不当默认）：** `--exclude <pattern> --exclude <pattern> --include <pattern>` 可多次传参；写法与逻辑参考 `.gitignore`（相对 import 根的模式匹配，后写规则覆盖先写规则）。

```text
import  dir/           -> 全量纳管 dir/ 及子目录
exclude dir/build/     -> dir/build/ 不再纳管
include dir/build/keep -> dir/build/keep 重新纳管（覆盖上一条 exclude 对该路径的判定）
```

exclude/include 的匹配语法已由 D-17 确定为 `.gitignore` 式（相对 import 根的模式匹配，后写覆盖先写）。

**以下三项代理候选值已由主人确认：**

- 诊断名：`missing`（磁盘文件不在且无 tombstone）/ `dangling`（记录在但退出有效历史）/ `unreachable_link` / `conflict`，与 ED-16 的 `diagnostics.kind` 一致。
- 虚拟根持久化：作为 `.omd/` 中一个特殊的 commit 文件（如固定 ID 的 `root.toml`），唯一性靠单写者锁 + 首次 init 时创建保证。
- tag 命名冲突：tag 是项目内平面字符串，同名即同一个 tag（这是特性，新文件打同名 tag 自动受约束）；跨项目不共享，跨项目规则需显式引用对方项目的 tag。

exclude/include 语法与 rule 失败级别已由本节确定。

### D-18：查询命令——`omd log`、`omd tree`、全局 `--json`

**状态：用户明确。追溯：R-18；Q-11；D-11、D-14、D-16、D-17；ED-16。**

用户明确：

- **`omd log <commit-id>`：** 类似 `git log`，沿 previous_id 链展示该 commit 的历史（commit ID、时间、操作、理由）。记录在的 commit 都可以查看，包括 dangling 和 tombstone 的 commit——这也是 D-17“重放 = 打开看信息后手工重做”的查看入口。
- **`omd tree [<commit-id>] [--level ...]`：** 类似 bash 的 `tree` 命令，显示挂载树的层级关系。不给 commit-id 时缺省从虚拟全局 root 开始；给出 commit-id（如某文件 commit）则从该节点开始。可限制层级，例如只到 project 或只到 file；不限制时一直显示到 range。
- **全局 `--json`（用户确认）：** 所有命令都可以 `--json` 输出 JSON，tree 尤其如此，以便与脚本集成。沿用 ED-16 的统一 envelope。

```text
omd tree --level file

root
├── proj/A
│   ├── [import] import -> exclude -> include
│   └── src/main.rs  (B0..B3)
└── proj/X
    └── docs/spec.md (Y0..Y1)
```

**边界：** tree 显示的是挂载层级（虚拟根 → 项目根 → import 分支/文件节点 → range），不是 link 引用关系；两者是不同遍历域（D-14）。

**工程候选默认（实现时可修订）：** 裸 `omd log`（无 commit-id）按时间倒序列出最近的 commit。

### D-19：迁移后范围保持、note 修订链、显式 gc、reset 目标语法

**状态：迁移后范围保持确认、note 修订链、显式 gc、reset 目标语法均已确认。追溯：R-06、R-08、R-09、R-18；Q-02、Q-09；D-09、D-11、D-14、D-15、D-16、D-17。**

用户明确：

- **迁移（rename）后原节点上的已确认 range commit 保持确认，link 关系不变，只是 path 从 source 变 target。** diff 是 verify 的时候跑的，commit 就只是 commit，这俩没关系。迁移不产生重新 diff，也不改变任何 commit 的内容或标记。
- **note 只增不改不删。** 每个 note 有自己的 note id。修改和删除不是原地操作，而是特殊的“delete note”或“patch note”——即在同一个 note id 上追加一个新 note。修改就是一个 patch 加到 note id 上的一个新 note。原始 note 保留，修订历史可追溯。
- **dangling commit 的长期保留策略：显式 gc 命令。** 由用户主动触发清理，不自动到期。具体命令名、清理范围与保护规则尚未冻结。
- **reset 目标语法：仅完整 commit ID（或唯一前缀），不提供 `HEAD~1` 之类的父提交简写。** OMD 的链模型简单，直接用 ID 最明确。

### D-21：unclean 展示、gc 保护规则、force 取消

**状态：unclean 展示与 gc 保护规则已确认；force 语义主人不记得设计初衷，需重新讨论。追溯：R-08、R-18；Q-05、Q-06；D-06、D-09、D-14、D-19。**

用户明确：

- **多次 unclean 的展示：平面列出全部。** 每条 unclean commit 独立存储独立列出，按时间序平面展示，全部须处理。与 note 的平面数组一致，不做合并计数。
- **gc 保护规则：不可达且无引用才清理。** 一个 commit 被 gc 清理的充要条件是：从虚拟根不可达，且没有任何有效 commit 引用它。被引用的 dangling 保留（诊断仍可读）。commit 被清理时其 note 一并清理。具体命令名仍为工程默认。
- **force 关联（D-06）已取消：** 主人确认取消 force，改用显式更新登记——用户自己设置/修正别名即可。remote 不匹配时 verify 报错并提示，用户通过更新别名登记解决，不提供运行时 force 覆盖。

### D-20：多用户冲突不提供解决工具；commit 支持显式时间戳参数

**状态：用户明确。追溯：R-18、R-20；D-11、D-14、D-17、D-19；ED-09。**

用户明确：

- **多用户冲突场景：** 如果两个用户在同一个 OMD 工作，可能产生冲突（各自写入了不同的后继 commit）。
- **OMD 不提供冲突解决工具：** 不做自动合并、不提供 mergetool、不比较两条分歧链的优劣。
- **commit 增加显式时间戳参数：** commit 支持 `--时间戳` 类参数（CLI 形式实现时定，如 `--timestamp`），用户可显式指定该 commit 的时间戳。指定值照常参与 hash（D-11 公式中的 timestamp 字段），确定性不变。
- **用途是手动重放解决冲突：** 用户查看对方历史（`omd log`）后，用指定时间戳在本地重建相应的 commit 序列，形成自己想要的历史顺序。这与 D-17 “重放 = 查看后手工重做”一致，`--时间戳` 是手工重放的必要工具。

**边界：** 时间戳参数不改变单写者锁（ED-09 仍防同机并发写），不改变“expected hash 冲突拒绝、不自动合并”（R-18）。时间戳的真实性由用户负责，OMD 不验证它是否接近真实时间；冲突谁对谁错由用户判断。

### D-17：link 只连接 range commit；commit 子命令别名体系；脏状态按 commit 而非位置判定

**状态：用户明确。追溯：R-08、R-09、R-11、R-18、R-20；Q-01、Q-02、Q-04、Q-07、Q-08、Q-11；D-01、D-03、D-11、D-14、D-16。**

#### range commit 的新建与续改

用户明确：range 节点（文件下的字符/字节跟踪节点）直接由 `commit --range` 初始化：

- **无 `--id`：** `commit --range <范围>` 新建一个 range commit，即新跟踪节点的首个 commit。range commit 可以相互重叠；**完全相同的 range 也可以打多个并行的 commit**（各自独立成链）。
- **有 `--id`：** `commit --id '<range commit id>' --range <范围>` 在该既有 commit 的基础上 commit；`--range` 此时意味着修改该跟踪的范围。

由此直接推出：range 跟踪节点的身份不是坐标本身（坐标可重复、可并行、可修改），而是**从首个 range commit 开始的 commit 链**；节点标识经由 commit ID 链确定。

#### link 的对象是 range commit

用户明确：link 也是 link range commit 而非文件。**不可以直接 link 一个文件到另一个文件，只能 link range commit 到另一个 range commit。** link 是 commit 下的子命令。

由此直接推出：D-03/D-04/D-07/D-08 中的“A → B”传播边，实际连接的是 A 的某个 range commit 与 B 的某个 range commit；文件节点之间没有直接的传播边。D-14 中“b1 引用 a1”的 a1/b1 也是 range commit。早先文档以文件名代指节点，是示例简化，不改变“传播只在 range commit 之间”的约束。

#### 脏状态按 commit 而非按位置判定

用户纠正代理候选默认“同一位置同时有脏和已确认 range 时该位置算脏”：脏不是按位置，而是**按 commit 的 range 来确认**。如果 diff 确认某一行是变动，那么覆盖该行的相关 commit 都会变脏。用户可以用 `commit clean --no--reason` 取消脏状态（D-10），并用截断传递（D-03/D-04 的源端阻断）去阻止脏状态传播。不引入“位置级”的合并规则。

#### 没有 Myers 基线存储这个概念

用户纠正代理候选默认“Myers 基线内容按 content hash 去重存储”：**不存在这玩意**。Myers 只是在 diff 时用来计算哪些 commit 需要标记为脏的算法。这不改变 R-20 “Myers 所需的旧内容必须可恢复”的需求：旧内容的来源是 range commit 自身记录的内容（D-11 的 content 字段），不是一个独立的“基线对象库”。

**代理推论（主人否决即改）：** diff 的两个输入是 range commit 自身记录的内容与当前文件内容；Myers 比较结果决定哪些 range commit 变脏。

#### commit 子命令别名体系

用户明确：import / remove / delete / init 都是 commit 的别名：

```text
import <dir>   == commit import <dir>    文件夹层级计入覆盖率统计范围（全量，见 D-16 tag）
remove <dir>   == commit remove <dir>    退出该文件夹的覆盖率统计整体；不删除已在跟踪的文件/range，历史保留
init <file>    == commit init <file>     文件层级初始化（D-15 的 B0）
delete <file>  == commit delete <file>   对应 init，即 tombstone（D-16 的 target: null）
link           == commit link            range commit 之间建立传播边
clean / unclean / verify                 已在 D-03/D-09/D-16 定义
```

remove 与 exclude 的关系（用户明确）：remove 退出的是**检查百分比的整体**——它不会删除已经在跟踪的文件或 range，历史保留可查；exclude 只是在 import 范围内层叠排除子路径。import 和 remove 只为统计百分比和打 tag（见下）服务，没有别的用途，不直接管理文件或 range。

#### tag 与 rule（用户明确，Q-06 的解答）

规则是**命名的检查集合**。用户可以在 import 的时候或之后用 `commit tag` 的方式给某个文件夹或文件打 tag（例如标记“这里是 spec”）。然后用 tag 之间的 link 方向约束来定义规则：

```text
omd rule 单向 link <tag> <tag2>     # tag 必须单向 link 到 tag2
omd rule 双向 link <tag> <tag2>     # tag 与 tag2 必须互相 link
```

规则基于 tag 而非具体文件，因此新增文件只要被打上 tag 就自动受同名规则约束。规则可以提醒或使检查失败（R-06），不要求声明时立即完成全部实现。

**失败级别（用户明确）：** 规则声明时指定级别，如 `--level=warn|fail`，默认 fail；warn 只提醒不使 verify 失败。一条规则一个级别，声明后可用 commit 修改。

#### 环与传播（用户明确）

range commit 之间的 link **允许成环**。脏状态传播按 D-14 的截断规则走：已访问的 commit 不再重复传播，环自然终止。commit link 时不做环检测、不拒绝成环。

#### import commit 的挂载归属（用户明确）

import commit 挂在 **project root 下，是一个单独的分支**，因为它不管理文件 commit。即：project root 下既有文件节点的 init/commit 链，也有独立的 import/exclude/include/tag 分支链；两者同一 project root 但互不混链。

#### dangling 是状态，不是命令

用户纠正：dangling 是一个状态，不是命令。代理在 Q-11 候选默认中列出的 `dangling` 子命令撤回。D-14 中“列出悬空提交”的功能仍保留。

**入口（用户明确）：** `omd list --dangling`。CLI 二进制名为 `omd`。

#### 其他纠正（针对代理候选默认）

- 无效 UTF-8：不是“拒绝作文本模式”，而是按已有决定（AGENTS.md 来源契约）由用户通过 CLI 参数或用户配置指定编码。
- 引用文法中路径只会存在 `/`，不会有反斜杠；代理候选默认中“反斜杠按字面处理”的表述不适用。
- 空文件覆盖率显示 **100%**（逻辑上按 100% 处理，尽管实际是 N/A），避免视觉不规整。

### 多上游、多下游与冲突拒绝

```text
A --+       +--> C
    +--> B -+
X --+       +--> D
```

A、X 的变更已经分别提交，当前内容版本为 A1、X1，B/C/D 尚未适配。A1、X1、B1 等是本例的版本简称，不是 CLI ID、Git 提交或最终存储字段。下表每次成功写入都另行读取并校验该次操作依据的元数据版本；不能在前一步修改元数据后，仍拿同一个旧 hash 连续写入。

| 步骤 | 操作前未处理事项 | 操作及所依据版本 | 操作后未处理事项 | verify 预期 |
| --- | --- | --- | --- | --- |
| 起点 | A1→B、X1→B | A/X 已提交；准备分别处理 | 不变 | 不通过 |
| 1 | A1→B、X1→B | 准备提交 B 对 A1 的适配；读取候选内容后，B 又被编辑，实际 hash 与本次依据不符 | 操作拒绝；不保存该次 OMD 提交，不消除 A1→B，也不以失败操作产生下游提交影响。B 的实际文件改动仍保留 | 不通过，并报告冲突/未提交变化 |
| 2 | A1→B、X1→B；B 有实际改动 | 用户重新读取并复核，显式提交当前 B1；仅选择 A1 的影响，版本校验通过 | X1→B、B1→C、B1→D | 不通过 |
| 3 | X1→B、B1→C、B1→D | 在 B 端针对 B1 的到 D 分支执行 commit clean，说明本次修改不影响 D；使用当前有效依据 | X1→B、B1→C | 不通过 |
| 4 | X1→B、B1→C | C 完成对 B1 的适配并提交 C1，明确选择 B1 的相应影响；本例 C 没有出向关联 | X1→B | 不通过；无需回 B 对已经适配的 C 补 clean |
| 5 | X1→B | 在 X 端针对 X1 的到 B 分支执行 commit clean，说明 B 无需为 X1 修改；核对当前依据，不复用过期读取结果 | 无 | 本例传播检查满足；当前来源采集、覆盖及其他已启用规则也满足时才能整体通过 |

第 1 步拒绝的是 OMD 写入，不是回滚用户在编辑器中的源码改动。第 2 步是使用者重新复核后发起的新操作，不是工具自动换 hash 重试。

没有出向关联的 C 不会凭空产生传播目标，也不需要为了凑 clean 记录添加一次空操作。这不豁免工作流中明确要求 C 存在某类关联的规则；如果该规则缺口存在，整体检查仍不能通过。

此路径在 A/X 的初始提交之外，有四次成功的处理写入：B 的适配提交、B 到 D 的阻断、C 的适配提交、X 到 B 的阻断。读取、校验和失败重试另计；不添加上游回填确认、下游代上游解释或整图盖章步骤。工具不以实际修改顺序推断语义正确，使用者对所选择的适配关系和理由负责。

### reset 产生悬空引用后的 broken 修复

**已确认的行为实例，未执行产品验收。** 假设 A 有 a0、a1 两次提交，B 的 b1 明确记录已经适配 a1，且 B 的旧提交 b0 所需信息仍全部可从当前有效根索引。

| 操作 | 当前有效记录与磁盘数据 | verify / 下一步 |
| --- | --- | --- |
| 初始 | A 当前为 a1；B 当前为 b1，引用 a1 | 假设原关联有效 |
| A reset 到 a0 | a1 仍在磁盘上，但已从当前有效根不可达；b1 及其评论仍在，仍引用 a1 | 警告 B 的 b1 为 broken（脏），列出悬空 a1，并提示可 reset 到 b0 后重建；不能报 all cleaned |
| 查看悬空提交列表及 a1 | a1 可列出并读取，当前有效状态不改变 | 仅查看不修复 B，不自动重放或清理 |
| B reset 到 b0 | b0 成为 B 的有效依据；旧 b1 与评论不被 reset 删除 | 该悬空引用问题解除；若来源内容与 b0 不一致或有其他待处理项，仍须处理 |
| 在当前有效依据上重新 commit | 生成新的 b1-new，而非改写旧 b1；必要引用均可从当前有效根索引 | 本例引用问题已修复；仍须通过内容、传播、覆盖等其他检查才能整体通过 |

D-15 已明确 B0 是文件节点的初始化 commit，且初始化只登记文件自身，不带上游适配依赖。迁移延续节点身份，reset 可沿迁移链选择目标（见「迁移链与回退目标」）。本例以 B0 的挂载路径与必要依据完整为前提；上游适配在后续提交中另行记录，不会把初始化提交一并判为 unreachable_link。找不到合格回退目标时仍如实报告，不伪造目标；是否还存在该情形及其恢复方式，要结合初始化与迁移契约继续核对。

**延伸到间接引用，用户已确认：** 假设 C 的 c1 依赖 b1，原先必要依据完整，且无其他未处理项。

| 时点 | C 的必要依据 | verify 对 B / C 的判断 |
| --- | --- | --- |
| A reset 前 | c1 依赖 b1，b1 依赖可达的 a1 | 本例无引用完整性问题 |
| A reset 到 a0，B 尚未 reset | b1 仍可索引，但其必要依据 a1 已 dangling | B 与 C 均为 unreachable_link；C 的诊断展示 `c1 --> b1 --> a1 (dangling)` |
| B 随后 reset 到 b0 | c1 仍引用旧 b1，而 b1 已悬空 | C 仍为 broken；B 的这项引用问题解除不代表 C 自动恢复 |

直接引用可达并不足以证明依据完整；间接断裂的诊断也不把仍可达的 b1 自动变成悬空提交。上述检查不修改 c1 的原引用，不自动 reset C。

此修复不要求再到 A 补 clean，也不允许以 clean 阻断代替回退重建。来源文件不由 reset 恢复，文件内容检查与引用完整性检查分别报告。

### 迁移链与回退目标

**状态：迁移延续节点身份、reset 可跨迁移链、迁移 commit 含 reason、verify 叶子冲突检测均已确认。追溯：R-06、R-09、R-18、R-20；Q-04、Q-09；D-09、D-14、D-15。**

用户明确：迁移延续同一节点身份，不是路径换行即开始新线路。reset 可沿迁移链选择目标：只要迁移 commit 本身的直接和间接必要依据完整，它就是一个合法的 reset 目标，与其 target 路径是否仍是当前磁盘位置无关——reset 不修改源文件，只移动跟踪状态的有效位置。

```text
B0 (init, target B) --迁移--> C0 (source B, target C) --适配 a1--> C1
                                                                  ^
C1 因 a1 dangling 而 unreachable_link。C 的合法 reset 目标包括 C0 与 B0，前提是各自必要依据完整。reset 到 B0 后，跟踪状态回到 "该文件位于 B"；磁盘上文件仍在 C，这一差异由内容检查单独报告。     a1 (dangling)
```

**迁移 commit 的不可变输入：** 迁移 commit 除 path.source / path.target 外，也包含 reason 字段（每个 commit 都有），属于不可变操作输入，参与 commit hash。

**叶子冲突检测（用户明确）：** verify 会整理每个最新 commit 真实指向的文件路径。如果两个不同的节点叶子都指向同一个文件路径，报告 **conflict** 状态——这也是 broken 的一种。conflict 与 dangling / unreachable_link 并列，作为 dirty 的诊断原因。

### 无理由阻断与外挂评论

对已有 commit c1，用户可通过 note 连续补充说明 n1、n2，形成 `notes [n1, n2]`。两条 note 都直接挂在 c1 上，不把 n2 作为 n1 的子回复；c1 的 ID 和传播效果不因评论变化。

用户若要显式停止当前所选分支，可创建 `commit clean --no--reason`。这是一条新的、有独立 ID 的 clean commit；仅省略理由，并不省略阻断操作或其版本校验。已有 note 不自动代替 clean，clean 也不被 note 操作隐式生成。

### clean、note、unclean、reset 的组合

假设本例所选范围没有其他未处理影响，来源内容从头到尾保持同一版本 H，初始有一条可以显式阻断的传播分支：

| 步骤 | 操作 | 有效跟踪状态 | 提交与评论 |
| --- | --- | --- | --- |
| 1 | 创建 c1：commit clean --no--reason，阻断该分支 | 该分支不再要求下游适配 | c1 是有 ID 的 clean commit，明确省略理由 |
| 2 | 给 c1 添加 note n1、n2 | 不改变步骤 1 的传播结果 | c1 的平面数组为 `[n1, n2]`；不产生状态 commit，不改变 c1 ID |
| 3 | 创建 c2：对同一选定范围 commit unclean，提供理由 | 新增必须处理的脏状态，即使内容仍为 H | c2 与 c1 是不同 commit，不能以 H 相同合并掉 |
| 4 | reset c1 | 有效状态移到 c1，即保留 c1 的阻断结果，c2 不再生效；来源内容仍为 H | 不创建反向状态 commit；c1 上的 n1、n2 不受影响，c2 也不被物理删除 |

此例可以由既有决定确定：c2 生效时存在待处理脏状态；撤下 c2 后不再因 c2 要求处理。整体 verify 仍须检查其他覆盖、关系和来源条件，不能只看 c1 存在便无条件通过。

若 c2 上也有 note，reset 不删除这些评论；在记录未被另行清理时仍可按 c2 的 ID 查看。之后新增 commit 以本节点有效前驱、新时间戳和随机盐按 D-11 生成新 ID，不把 c2 的 ID 转用于新提交或将 c2 的评论挂到新提交上。

### 最小记录需求：信息必须够用，不等于增加服务

以下是从已确认行为推出的信息需求，不是已选定 schema。可以先由权威文本记录、必要基线和可重建索引满足；不因此增加队列服务、后台调度器、独立事件数据库或另一套生命周期。

| 必须能回答的问题 | 需要保留的信息 | 缺少时的错误 |
| --- | --- | --- |
| 这次修改了哪里、依据什么内容？ | 来源/范围、坐标模式、相关旧/新内容版本及可恢复基线 | 只有 hash 无法 diff，或坐标跨版本误用 |
| B 的提交具体处理了哪些上游影响？ | 明确选择的来源变化与关联，以及本次 B 内容版本 | 一次 B 提交误消除所有上游影响 |
| 本次变化还需要处理哪些下游？ | 相关出向关联及各自所依据的变更版本、已发生的适配或阻断记录 | 一个分支处理后掩盖其他分支；旧阻断错误沿用 |
| 为什么在这里停止，或是否显式省略理由？ | 当前源端、选定分支、对应变化，以及理由或显式无理由选择 | 将 clean 变成永久豁免、完成盖章，或伪造理由 |
| 使用者读取后又有人改了什么？ | 本次操作的 expected hash 前置依据 | 覆盖他人修改或悄悄换版本重试 |
| 内容未变但跟踪状态是否变了？ | 与内容 hash 区分的状态版本、显式状态操作，以及理由或准许的省略选择 | unclean 被忽略，或回滚后仍使用过期索引 |
| 评论挂在哪次提交上？ | 本节点 16 字符盐、前驱、时间戳、受管内容和提交元数据派生的 commit ID，以及附属 note 的有序平面记录 | 同内容提交被错误合并、前驱跨节点误绑、操作元数据遗漏、评论挂错提交或修改评论导致 commit ID 改变 |
| 数据仍在但当前引用是否有效？ | 当前有效根/历史索引、磁盘提交清单、必要引用及可回退依据 | 把磁盘存在当作有效，遗漏 broken，或让诊断遍历意外恢复悬空提交 |

检查可以从这些权威记录派生待处理事项。是否另存派生状态只是实现选择，不得使删除 SQLite 后丢失唯一的适配、阻断、理由或版本依据。具体记录拆分、ID 形式、hash 粒度和崩溃恢复仍在 Q-09，不在本节冻结。

### 端到端走查（最终模型）

**状态：纸面推演，组合 D-01 至 D-19 与 ED 清单，不新增行为，不是产品测试记录。** 用最终模型（range commit 才会变脏、import 只管统计与 tag、link 只连 range commit）重走完整生命周期，验证各决定拼接后无矛盾。

| 步骤 | 操作 | 依据 | 结果 |
| --- | --- | --- | --- |
| 1 | `omd commit import docs/` | D-02/D-16/D-17 | docs/ 计入覆盖率统计范围；不创建任何文件节点 |
| 2 | `omd commit init docs/spec.md` | D-15/D-16 | 文件节点 B0（仅 path.target）；无 range，覆盖率呈未标记缺口 |
| 3 | `omd commit --range 0:120 docs/spec.md --reason "审阅"` | D-03/D-17 | 新建 range commit 链 r1，内容为所选范围文本 |
| 4 | `omd commit tag docs/spec.md spec` | D-17 | 打上 tag spec，受同名规则约束 |
| 5 | 磁盘编辑 spec.md 后 `omd verify` | D-16/D-17 | hash 变化；Myers 以 r1 记录内容对比当前文件，命中行覆盖的 r1 变脏；未命中 range 保持已确认 |
| 6 | `omd commit --id r1 --range 0:128 --reason "适配"` | D-17 | r1.1 追加在 r1 链上，范围可调，重新确认 |
| 7 | 全部脏 range 解决后 `omd commit verify docs/spec.md` | D-16 | 产生 hash 变化 commit B1，记录新 hash |
| 8 | `omd commit link <r1.1> <c-range> --reason "对应"` | D-17 | 建立 range→range 传播边；不连文件 |
| 9 | 上游再变，`omd verify` | D-03/D-14/D-17 | 沿 link 传播脏到 c-range；截断防环；可用 `commit clean --no--reason` 在源端阻断 |
| 10 | `omd reset <B0>` | D-09/D-16/D-19 | B0 之后的 B1 及级联的 r1 链退出有效历史成 dangling；引用它们的节点报 unreachable_link |
| 11 | `omd list --dangling`、`omd log <id>` | D-17/D-18 | 查看悬空记录；重放 = 查看后手工重建 |
| 12 | `omd commit delete docs/spec.md` | D-16/D-17 | tombstone（target: null）；依赖方 broken；历史与 note 保留 |
| 13 | `omd gc` | D-19 | 显式清理不可达记录；范围与保护规则见实施期待定清单 |
| 14 | `omd tree --level file --json` | D-18/ED-16 | 挂载层级 JSON 输出，供脚本集成 |

**走查中明确的对齐点：**

- “verify”一名两用：`omd verify`（检查，D-05）与 `omd commit verify <path>`（产生 hash 变化 commit，D-16）是不同位置的不同操作，保留主人原话，不合并。
- 文件节点 reset 级联到其下 range 链：D-16 “父退出子不可达”同样适用于文件→range 挂载层。
- 范围 commit 由用户带理由提交，本身即为已确认标记（推论）：与 D-15“init 不赋予已确认”不矛盾——init 是文件节点登记，range commit 是审阅动作。

### 当前收敛程度

- **足以写入行为规格的部分：** 端到端生命周期（import → init → range commit → verify 标脏 → 适配 → commit verify → link → 传播/阻断 → reset 级联 → dangling 查看与手工重放 → tombstone → gc → log/tree 查询）已按最终模型纸面走查无矛盾，见上节。单次变化沿直接关系逐步处理、版本冲突拒绝、unclean 层叠、迁移链回退、broken 修复路径等场景表仍然成立——其中的“A1→B”按 D-17 重新解读为 range commit 之间的边。
- **实施期待定清单（集中记录，均为实现期细节，不改变已确认行为）：**
  - gc 的具体命令名、清理范围与保护规则（D-19 只定了“显式 gc 命令”）；
  - 多次 unclean 的计数与合并展示方式——已由本节确定为平面列出全部（D-21）；
  - `--force link` 的完整参数位置——已由 D-21 取消，改用显式更新登记（设置/修正别名）；
  - note 作者字段（D-11/D-19）；
  - command 执行上下文：cwd、stdin、env/PATH、超时、输出上限与信任边界（Q-05 余量）；
  - 回退目标查找的具体算法（可达性定义已由 D-16 给出）；
  - 节点标识的具体格式（D-15 尾注）；
  - 裸 `omd log` 的行为（D-18 工程候选默认）；
  - Myers diff 输入是“range commit 记录内容 vs 当前文件”的代理推论（D-17，主人未否决）。
- **不在本变更内的同步缺口：** docs/ 原始文档（requirements、source-model、storage、open-questions、acceptance 等）仍保留旧框架与未决标记；当前授权仅覆盖本 change 目录的 proposal.md 与 design.md。specs/tasks 阶段必须先以本目录为准，或经主人授权同步 docs/。

## 工程默认清单（ED）

**状态：以下为代理提出的候选工程默认，主人在对话中逐项过目、未反对；其中与主人后续纠正冲突的条目已移除或改写。标注 ED 编号便于引用。实现时如发现与已确认决定冲突，以已确认决定为准并修订此处。它们不是逐条用户批准的产品行为，是工程默认值。**

**坐标与编码（Q-07）：**

- ED-01 索引从 0 开始，区间左闭右开 `[start, end)`。
- ED-02 换行符按原文字节保留，不做 CRLF/LF 归一化；BOM 计入第 0 个 code point。
- ED-03 编码优先级：范围声明 > 文件级配置 > 项目级默认 > 全局默认 > UTF-8。无效 UTF-8 的处理以 D-17 为准：由用户 CLI 参数或配置指定编码。
- ED-04 引用文法中 `::` 用 `\::` 转义；路径只存在 `/`（主人确认）。

**覆盖（Q-08）：**

- ED-05 重叠 range 按并集计算覆盖长度，不重复计数。位置级脏合并没有被采用，脏判定以 D-17 的按 commit 为准。
- ED-06 分母 = 文件解码后总 code point 数（byte 模式为总字节数）；不跳过空白。空文件覆盖率显示 100%（D-17）。

**持久化（Q-09）：**

- ED-07 权威文本用 TOML，每个 commit 一个文件，文件名即 commit ID 的十六进制。
- ED-08 expected hash 前置条件同时校验元数据版本和来源快照 hash。
- ED-09 单写者锁用目录级 lockfile（pid + 时间戳）；陈旧锁由用户显式 `unlock`，不自动抢占。
- ED-10 多文件提交按顺序写入，最后写 manifest；崩溃时 manifest 缺失视为未完成，下次启动报告并要求用户选择丢弃或续写。
- ED-11 hash 算法 SHA-256；commit ID 显示为 64 位十六进制，CLI 接受唯一前缀。
- ED-12 盐字符集 `[A-Za-z0-9]`，来自 OS CSPRNG；时间戳 RFC 3339、UTC、毫秒精度。

**路径发现（Q-10）：**

- ED-13 `OMD_CONFIG_PATH` / `OMD_CACHE_PATH` 为空字符串等同未设置；相对路径相对 cwd 解析；指向不存在或不可写路径时报错退出，不回退。
- ED-14 项目根识别：显式 `--root` > 向上查找最近的 `.omd/` > 报错，不猜测。
- ED-15 非默认元数据目录只扫描项目根直属一级子目录中含 `omd-manifest.toml` 的目录；多个候选报歧义错误。

**CLI（Q-11）：**

- ED-16 `--json` 输出统一 envelope `{ ok, data, diagnostics[] }`；diagnostics 项含 `kind` / `node` / `commit_id` / `message`。所有命令支持 `--json` 已由主人确认（D-18）。
- ED-17 退出码：0 通过，1 检查未通过（有脏/broken），2 用法/参数错误，3 版本冲突拒绝，4 锁冲突。
- ED-18 Git hook 检查工作树文件，文档明确说明不检查 index；安装时已有同名 hook 则追加调用而非覆盖。

**项目设置（Q-12）：**

- ED-19 许可证、测试组织、发布方式留到实现启动时决定。

## Risks / Trade-offs

- **纯位置变化也要复核，操作量可能增大。** 缓解：提供候选位置和可显式选用的固定英文理由，不偷换成自动语义放行。
- **将 clean 做成永久节点布尔值会掩盖后续变更。** 缓解：只对具体变更与选定作用域有效，继续遵守旧 hash 冲突即拒绝。
- **推进全局文件基线可能抹掉其他分支的未完成事项。** 缓解：保留未处理关系的确认依据与可恢复内容；具体布局按 Q-09 继续确定。
- **把 clean 当作完成盖章，会迫使适配提交后再重复确认。** 缓解：按 D-03/D-07 区分提交接续与源端阻断，按 D-08 显式选择所处理的上游变化；具体参数仍须规格化，不能凭任意 commit 清除所有影响。
- **reset 后其他节点可能引用悬空提交。** 按 D-14 报 broken（脏）并提示 reset 到依据完整的 commit 后重建；不能因数据仍在磁盘而通过，不能用 clean 绕过，也不自动修改源文件或恢复悬空提交。根索引、重放及安全清理仍需细化。
- **评论或理由修订与状态操作混淆。** 缓解：note 只作外挂说明；commit clean 即使省略理由也仍是显式状态操作，两者不能相互冒充。
- **固定 command 参数不保证输出稳定或没有副作用。** 缓解：默认不执行，遵守显式配置/参数；同次已采集版本用于后续清理，不为 hash 校验重复运行。
- **持续 import `.omd/` 可能使自身写入产生新变化。** 缓解方向仍需 Q-04/Q-09 规格化；不得隐藏排除、自动确认或承诺自引用必然收敛。
- **Git remote 身份核对增加运行时失败路径。** 缓解：保持可选身份约束与内容核心分离；force、缺失 remote 和比较规则未决，不用静默接受来消除错误。

## Migration Plan

没有运行中的产品数据需要部署或迁移。本次只补充探索记录，不将已存在文档整体重写，也不把 OpenSpec artifact 的 `done` 状态当作设计完整或测试通过。

继续细化相关 Q，确认后立即更新本文。创建 delta specs、实施任务或修改原有 `docs/` 须另有对应范围的授权；本轮不创建这些文件。其余未解决 Q 继续以原清单为准，不因为已有记录就视为全部解决。
