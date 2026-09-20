## Purpose

定义 OMD 如何把已记录的完整旧内容与真实文件当前路径的内容或获准采集的新 stdout 进行比较。Git 可提供确切的旧版本，不替代真实文件的当前观察。统一进行范围跟踪、变化检测及覆盖统计，同时按保存方式区分历史恢复保证；不把 import、路径变更、来源转换或文件 hash 更新当成已经完成范围复核。

> 规划基线：D-33 已确认末尾插入须复核并接受 E-1～E-8；本规格据此覆盖坐标、迁移、历史定位、replace 与写入契约。文件 reset 见 change-review。以下为尚未实现的要求与验收场景，不是产品测试通过记录。

## ADDED Requirements

### Requirement: Content sources share tracking behavior

系统 SHALL 支持 file、command、git 取得所需内容，不因此新增挂载层级或标记状态。比较时 SHALL 区分已记录的完整旧内容与当前观察：真实文件的当前内容读取登记路径现在指向的文件，包括未提交修改；历史内容是否由 Git 提供不改变这一点。虚拟 command 的当前内容仍通过获准执行取得完整成功 stdout，不要求它对应真实磁盘文件。旧内容按记录中的保存方式取回；git 只提供其确切 Git 版本的内容，不充当当前文件现状。

取得旧新内容后，系统 SHALL 共用内容 hash、Myers、坐标、range、link、适配、ATOMIC、reset、note 与检查规则，不另建跟踪流程。本文明确的 Git 历史保存例外及 command 执行许可仍 SHALL 遵守。

#### Scenario: Equal acquired contents follow equal review rules
- **GIVEN** 文件来源与 command 来源具有相同的完整旧新内容、坐标模式、范围及复核条件，且 command 已获准并成功采集
- **WHEN** 系统检查两者的变化
- **THEN** 按同一套规则判断受影响范围与待处理责任，不因内容来自 stdout 而自动确认或豁免
- **AND** 两个独立跟踪对象仍保留各自的身份，不因内容相同合并

### Requirement: Import statistics are separate from file tracking

系统 SHALL 仅在用户显式 import 的目录范围内统计该范围的覆盖情况，并持续发现范围内新增文件。import/remove SHALL 只改变统计范围及相关 tag 记录，不自动 init 文件、不创建 range、不删除独立跟踪的文件或 range，也不改动磁盘来源。import 分支 SHALL 与文件分支平行挂在项目根下。

#### Scenario: New document enters the imported statistics
- **GIVEN** 用户已 import `docs/`，目录中尚无 `intro.md`
- **WHEN** 用户在磁盘新增 `docs/intro.md` 并检查覆盖
- **THEN** 新文件进入该范围统计，其未标记内容显示为缺口
- **AND** OMD 不自动创建它的文件 init 或 range commit

#### Scenario: Remove does not erase tracking
- **GIVEN** `docs/` 在统计范围内，`docs/spec.md` 已有文件与 range 提交
- **WHEN** 用户显式 remove `docs/`
- **THEN** 该 import 范围退出统计
- **AND** 已有文件、range、历史和磁盘内容不被删除

### Requirement: Import has explicit scope adjustments and no hidden exclusions

目录 import SHALL 包含其子目录。用户 SHALL 能重复提供 `--exclude`、`--include` 模式以层叠调整统计范围，模式写法与逻辑参考 `.gitignore`。系统 MUST NOT 因目录名是 `.omd/` 而禁止用户显式 import 或偷偷排除它。系统 SHALL follow 符号链接，并在断链或循环时报告诊断，不将所有符号链接默认排除。

#### Scenario: User explicitly tracks metadata in statistics
- **WHEN** 用户显式 import `.omd/`
- **THEN** 该目录按显式选择进入统计，不因是元数据目录而被隐藏排除
- **AND** OMD 不自动确认自身写入造成的新变化，也不保证自跟踪收敛

#### Scenario: A symbolic link forms a traversal cycle
- **GIVEN** 已 import 目录中存在指回祖先目录的符号链接
- **WHEN** OMD 遍历该范围
- **THEN** 遍历终止并报告循环
- **AND** 不能以未完成的遍历报告完整覆盖通过

### Requirement: Files are initialized explicitly

系统 SHALL 用显式文件 init commit 登记文件；初始路径记录只含 `path.target`，省略 `path.source`。init SHALL 不与上游适配合并，不自动赋予任何正文范围确认资格。`init`、`import`、`remove`、`delete` SHALL 分别作为对应 `commit init/import/remove/delete` 操作的别名。

#### Scenario: Initializing a file is not a review
- **WHEN** 用户显式 init `docs/spec.md`
- **THEN** 产生该文件的初始化记录并保存目标路径
- **AND** 没有 range 标记的正文仍是未标记缺口
- **AND** 该 init 不声称已适配某个上游 range commit

### Requirement: Range identity is independent of coordinates

系统 SHALL 在 `commit --range` 未指定 `--id` 时创建独立的范围跟踪首个 commit；指定既有 range commit 的 `--id` 时，在该跟踪上提交范围变更，不原地覆盖历史。重叠范围和完全相同坐标的独立跟踪 SHALL 都被允许；系统 MUST NOT 仅按坐标合并其身份。

#### Scenario: Identical coordinates have independent records
- **WHEN** 用户在同一文件同一来源版本上两次提交相同范围，均不指定 `--id`
- **THEN** 得到两个独立的 range 跟踪记录
- **AND** 后续处理其中一个不等于处理另一个

#### Scenario: A range is extended explicitly
- **GIVEN** range commit r0 记录范围 10～20
- **WHEN** 用户指定 r0 的 ID，将范围修改为 10～30 并提交
- **THEN** 生成该跟踪的后续 commit r1，记录新范围
- **AND** r0 的坐标和记录保持不变

### Requirement: Full source versions survive cache loss

以 file 或 command 方式保存的来源版本 SHALL 保留对应完整文件或成功采集的完整 stdout；range SHALL 引用完整来源版本及坐标。这些完整内容 SHALL 能在无 Git、删除可重建缓存后恢复。git 方式的内容恢复 SHALL 按下方 Git 对象复用要求处理，而非默认为另有 OMD 内容副本。对任何来源，系统 MUST NOT 只凭 hash 或选中片段声称足以比较完整旧新内容；Myers 是差异算法，不另设独立的产品状态或标记生命周期。

#### Scenario: Repeated fragments retain their original context
- **GIVEN** 旧文件包含两处相同的 `abc`，range 记录的是第二处
- **WHEN** 后续 verify 比较当前文件
- **THEN** 差异计算使用 range 所引用的完整旧来源与当前完整来源
- **AND** 不只用 `abc` 片段猜测旧位置

#### Scenario: Rebuild without Git or cache
- **GIVEN** 目录不受 Git 管理，权威记录包含已提交的完整来源
- **WHEN** 用户删除查询缓存后重新检查
- **THEN** 旧内容和 range 依据仍可从权威记录恢复
- **AND** 不需要 Git blob、Git diff 或缓存中唯一一份内容

### Requirement: Git references identify historical contents without replacing current observation

用户 SHALL 能显式选用 git 来源，从本地仓库已有 Git 提交中取得所记录版本的完整内容。该历史依据 SHALL 保存确切 Git commit ID、仓库定位及版本内文件定位等必要信息，并可追溯到对应 OMD commit；Git ID 与 OMD ID SHALL 分别保留，不能互相替代，也不假定一一对应。浮动 ref 名不能代替历史版本的确切 ID。

真实文件的 verify SHALL 将记录中的完整旧内容与当前登记路径上的完整内容比较，不以 HEAD、某个分支、index 或 Git 状态列表选择或代替当前内容。未提交的 Y SHALL 被作为当前文件内容检查，但 MUST NOT 冒认成历史 G1 的 X。历史可读不能掩盖当前路径缺失，当前文件存在也不能替代丢失的历史对象。系统 SHALL 使用内容 hash 和 Rust 内 Myers，不以 Git diff 或 changed-files 替代；不能因使用 Git 历史而自动创建 Git 提交、切换工作区或获取远端。历史引用 SHALL 使用 `git::<JSON 对象>` 的 repo、commit、path 字段定位本地仓库、完整确切 Git commit ID 与版本内路径；校验对象类型并读取完整原始 blob，不执行 textconv/filter 或隐式 lazy fetch。历史符号链接 SHALL 在同一提交内解析并检测循环/断链，不回退工作树。

#### Scenario: Verify uncommitted changes after same-content replacement
- **GIVEN** O1 记录文件的完整内容 X，已通过同内容 replace 改由 Git G1 中的该版本提供，之后登记路径上的文件变成未提交的 Y，HEAD 仍为 G1
- **WHEN** 用户运行 verify
- **THEN** 比较从 G1 取得的完整旧内容 X 与从当前路径读取的完整 Y，按共同规则判断受影响范围
- **AND** O1 的 ID、原始输入、link 和 note 不变，不把 O1 改成 Y，也不因这次检查自动追加 OMD 或 Git commit

#### Scenario: Readable Git history does not hide a missing current file
- **GIVEN** O1 的历史内容 X 仍能从 G1 取回，但登记的当前文件路径已不存在且没有 tombstone
- **WHEN** 用户运行 verify
- **THEN** 报告当前文件 missing，不用 G1 的 X 充当当前文件或自动创建 delete commit
- **AND** 历史 X 仍可按原记录查看，不因当前文件消失而改写历史依据

#### Scenario: HEAD movement alone does not change the observed file
- **GIVEN** O1 的旧内容由 G1 提供为 X，当前登记路径的文件也为 X，而 HEAD 已移到保存不同内容 Y 的 G2
- **WHEN** 用户运行 verify
- **THEN** 比较 G1 的 X 与路径上的 X，不用 G2 的 Y 替换当前观察，也不因 HEAD 改变就标为内容变脏
- **AND** 既有 unclean 或其他待处理责任仍保留

### Requirement: Git source history reuses repository objects

以 git 方式引用的来源版本 SHALL 复用对应 Git 对象，不由 OMD 再备份一份完整内容；必要定位信息 SHALL 保存在权威文本中，而非仅存在 SQLite 缓存里。删除 OMD 缓存而仓库对象完好时，旧完整内容 SHALL 仍可取得。仓库或对应对象丢失时 SHALL 报告来源版本不可取得，不能以 hash、工作区当前内容、另一个 Git 版本或自动 fetch 冒充恢复成功。该读取失败 SHALL 是诊断，不是新的标记生命周期。

该依赖 SHALL 明确限定于采用 git 保存方式的版本，不改变 file/command 版本的完整内容保存要求。仅记录 OMD 引用 MUST NOT 被宣称为已经保护 Git 对象免受清理，也不授权 OMD 自动执行 git gc 或创建保护 refs。

#### Scenario: Git history remains readable after OMD cache loss
- **GIVEN** 权威记录保留某来源版本的 Git commit ID 与必要定位，所需仓库对象仍存在
- **WHEN** 用户删除 OMD 查询缓存后查看该版本
- **THEN** 能从指定 Git 对象取得完整旧内容，不需要缓存中的唯一副本

#### Scenario: Missing Git objects cannot be replaced by current working-tree content
- **GIVEN** OMD 仍保留 Git 来源记录，但其所需 Git 对象已经丢失，工作区另有文件内容
- **WHEN** 操作需要读取该历史版本
- **THEN** 报告版本不可取得，不用工作区内容冒充，也不自动下载或声称已完成比较

### Requirement: Source replacement preserves the complete recorded version

系统 SHALL 支持 file、command、git 之间的显式 replace，不通过新增 `commit set` 或其他 OMD commit 叠加来源设置。新来源 SHALL 提供被替换版本的同一份完整内容；只有选中 range 片段相同不够。完整内容不一致时 SHALL 拒绝本次 replace，并保持原来源绑定、内容与历史不变；MUST NOT 降级为只切换后续读取入口。

成功 replace SHALL 保留既有跟踪对象、OMD commit ID、原始不可变操作输入、range、link ID 及 note。它改变所选已记录版本的内容取回方式，不改变该版本的内容，不替用户提交 Git。对真实文件，转用 Git 历史引用 MUST NOT 将当前观察改成读取 Git 树或忽略登记路径上的未提交变化。其他历史版本及共享内容仍需各自的有效恢复依据；不能因为转换一个版本就将其他版本一律指向同一个 Git 提交，或自行清除旧副本。来源读取许可、单写者及旧版本/hash 校验 SHALL 继续适用。目标选择、共享版本影响及副本释放 SHALL 遵守下方来源版本绑定要求。

#### Scenario: Move an existing file-backed version to matching Git content
- **GIVEN** O1 记录完整内容 X，由文件副本提供，本地 Git 的 G1 在指定位置也保存完整 X
- **WHEN** 用户请求 replace 到该 git 来源，且内容读取与写入前置检查通过
- **THEN** O1 的 X 改由指定 Git 内容取得，可查到 Git/OMD ID 关联，不新增 OMD commit
- **AND** O1 的 ID、原始输入、range、link ID 和 note 保持不变，不自动清除仍保留的副本

#### Scenario: Equal range snippets cannot authorize replacement of different full contents
- **GIVEN** O1 保存完整内容 X，目标来源提供不同的完整内容 Y，即使被选 range 的片段相同
- **WHEN** 用户请求 replace
- **THEN** 操作被拒绝，原来源绑定、完整内容和历史不变，不仅切换后续读取入口

#### Scenario: Replacing one version does not discard an earlier unmatched version
- **GIVEN** O0 保存 Z，O1 保存 X，目标 Git 版本保存 X，但没有为 O0 提供对应 Git 内容
- **WHEN** 用户成功转换 O1 的来源
- **THEN** O0 的 Z 仍保留原有恢复依据，不被改挂为 X，也不因 O1 转换而丢失

### Requirement: Text and byte sources retain distinct coordinate units

系统 SHALL 采用 0 起点、左闭右开 `start:end`，允许空区间并检查 `0 <= start <= end <= 来源长度`。文本范围 SHALL 按用户指定编码解码后的 Unicode scalar value 计数；byte 范围 SHALL 按原始字节偏移计数且不解码。编码 SHALL 按本次显式 `--encoding`、此跟踪已记录编码、文件配置、项目默认、用户默认、UTF-8 的顺序选择，并保存每次使用的编码，不能以配置变更重新解释历史。系统 MUST NOT 混用两种坐标或静默替换无效字符。原换行、BOM SHALL 保留，BOM 计入文本第 0 个位置；引用中的路径使用 `/`。

#### Scenario: Multibyte text is not indexed as raw bytes
- **GIVEN** 文件中包含占多个 UTF-8 字节的汉字
- **WHEN** 同一文件分别被按文本和 byte 模式跟踪
- **THEN** 文本位置使用 Unicode 字符序号，byte 位置使用原始字节偏移
- **AND** 系统不能将一种模式的坐标不经区分地应用于另一种模式

#### Scenario: User selects a non-UTF-8 encoding
- **GIVEN** 文件不能按 UTF-8 正确解码，但用户明确指定了实际编码
- **WHEN** OMD 读取该文本来源
- **THEN** 使用指定编码解释文本范围，不强制按 UTF-8 解码

### Requirement: Verification identifies affected ranges without semantic claims

系统 SHALL 以内容 hash 检测来源变化，并在 verify 时用 Rust 内的 Myers 同类算法比较完整旧新来源，判断相关 range commit 是否受影响。文件提交本身 SHALL 负责记录来源 hash，而不是作为正文脏标记。纯位置变化 SHALL 提供候选迁移并要求用户显式复核，不能自动沿用确认。范围末尾紧贴插入内容 SHALL 使旧范围变脏，即使原文字和坐标未变，因为系统无法确认新增内容与旧范围无关；MUST NOT 自动扩张范围或确认新增内容。范围内修改及跨边界修改也 SHALL 要求复核。未被这些变化影响的范围 SHALL 保持原确认，不仅因整文件 hash 改变而变脏。系统 MUST NOT 将最短 diff、格式相似或文字相同当成语义无害证明。

#### Scenario: Local edit does not invalidate the entire file
- **GIVEN** 文件有两个互不相交且均已确认的范围，正文修改仅影响第一个；第二个内容和位置均未变，也未接触修改或插入边界
- **WHEN** 用户运行 verify
- **THEN** 第一个范围报告为脏，第二个保持原有效确认
- **AND** 不仅因文件 hash 变化就将所有 range 标脏

#### Scenario: An insertion at the end requires review without automatic expansion
- **GIVEN** `校验密码。然后返回结果。` 中已确认范围仅包含 `校验密码。`
- **WHEN** 当前内容变为 `校验密码。记录日志。然后返回结果。`，用户运行 verify
- **THEN** 原范围变脏，即使它的原文字与坐标未变
- **AND** 不自动扩大它的范围，也不确认新增的 `记录日志。`

#### Scenario: Content moves without changing its text
- **GIVEN** 已确认范围前插入了内容，范围正文未变但位置改变
- **WHEN** 用户运行 verify
- **THEN** OMD 提供候选新位置并要求显式复核
- **AND** 未经复核不能将自动迁移后的范围计作有效确认

### Requirement: File hash commits do not clear range obligations

用户 SHALL 能在该文件脏 range 已解决且验证通过后，以 `commit verify <文件路径>` 产生记录最新 hash 的文件 commit。文件 hash 更新 MUST NOT 自动消除未处理的范围或关联责任。普通 rename commit SHALL 不触发 Myers diff；差异判定由 verify 执行。

#### Scenario: Outstanding range work blocks a successful file verification commit
- **GIVEN** 文件内容已变化，其中一个 range 仍脏
- **WHEN** 用户请求为该文件记录通过验证的新 hash commit
- **THEN** 检查不能通过，也不能以新 hash 隐藏该 range 的责任

### Requirement: Paths can change without merging identities

系统 SHALL 通过用户手动提交的 `path.source` 和 `path.target` 记录 rename，延续原节点身份、已有范围确认及 link；OMD MUST NOT 主动改名磁盘文件或自动检测并登记 rename。复制文件 SHALL 使用自己的 init 建立新跟踪，不自动复制原节点身份或关联。

#### Scenario: Reusing a vacated path keeps histories separate
- **GIVEN** 原文件 B 已手动迁移到 C，随后原文件 A 手动迁移到 B
- **WHEN** 用户分别提交两次路径迁移
- **THEN** C 延续原 B 的历史，新路径 B 延续原 A 的历史
- **AND** OMD 不因路径同名而合并两者的范围、note 或关联

#### Scenario: Rename does not assert a content adaptation
- **GIVEN** 文件有已确认 range 与既有 link
- **WHEN** 用户只登记 source 到 target 的 rename
- **THEN** 路径记录变化，范围确认与 link 保留
- **AND** 后续实际内容差异仍由 verify 检查，rename 不替用户适配内容

### Requirement: Tombstones and missing sources remain distinguishable

显式 delete SHALL 创建 source 为原路径、target 为 null 的 tombstone，保留历史与 note，仍依赖该被删除对象的关系 SHALL 报 broken。磁盘文件消失而没有 tombstone 时 SHALL 报 missing。OMD MUST NOT 将路径不存在自动解释为有意删除，也不借此物理清除记录。

#### Scenario: An unrecorded deletion is reported
- **GIVEN** 活跃文件记录的 target 路径已从磁盘消失，没有对应 tombstone
- **WHEN** 用户运行 verify
- **THEN** 报告 missing，不自动生成 delete commit

### Requirement: Coverage uses only current confirmations

系统 SHALL 保持空、脏、已确认三种标记状态，只有未过期的已确认范围可贡献有效覆盖。`check` SHALL 对所选内容范围统计覆盖率；tag 关系的 link 覆盖按 local-project-links 的内容并集规则计算，不能仅枚举已有 range 而遗漏未标记内容。命令失败、断链和定位问题 SHALL 使用诊断表达，不新增标记生命周期。

文本覆盖统计 SHALL 使用 Unicode White_Space（Rust `char::is_whitespace`）排除空白位置；空白不计入分母，也不作为未覆盖缺口。跨文件同单位计数 SHALL 先加分子分母再计算百分比，不平均文件百分比；文本与 byte 不混加为无单位总百分比。空文件和全空白文本的覆盖结果 SHALL 按 100% 计算并显示。统计过滤 MUST NOT 改写来源内容、坐标、hash、Myers 的比较内容或 replace 的完整内容相等要求，也不能给 byte 模式套用文本空白过滤、擅自排除零字节。覆盖结果 MUST NOT 声称内容解释充分或业务语义正确。

覆盖不足 MUST NOT 被自动作为 verify 的失败条件；覆盖达到 100% 也 MUST NOT 清除独立待办或替代 verify 的变化、必要依据、来源及闭合检查。check MUST NOT 自动创建、确认或修复范围/link，也不因调用 check 就授予来源命令额外执行许可。

#### Scenario: Empty counted content has a stable displayed percentage
- **WHEN** 用户通过 check 统计空文件或全空白文本的覆盖率
- **THEN** 覆盖率显示 100%，不显示 N/A 或 0%
- **AND** 全空白文本的原始内容保留，不被改写成零字节文件

#### Scenario: Text whitespace is excluded from coverage but remains in the source
- **GIVEN** 文本依次为 A、空格、B、换行，A 和 B 均有合格覆盖，空格和换行没有覆盖
- **WHEN** 用户运行 check
- **THEN** 该文本的覆盖率为 100%，空白不进入分母或缺口
- **AND** 原文和坐标保留，hash、Myers 及 replace 仍使用完整内容，不把统计过滤当成空白修改的自动放行

#### Scenario: Incomplete coverage does not fail verification by itself
- **GIVEN** 应计内容 `ABCD` 中仅 `AB` 有合格覆盖，`CD` 是覆盖缺口，但 verify 自己的条件均满足
- **WHEN** 用户分别运行 check 和 verify
- **THEN** check 报告覆盖未达 100% 并保留 `CD` 的缺口，verify 在自身条件满足时通过
- **AND** verify 通过也不会补造 `CD` 的标记或关联

#### Scenario: Full coverage does not clear an independent obligation
- **GIVEN** 应计内容已由合格范围完整覆盖，另一个重叠的独立 range 仍有未处理的 unclean 责任
- **WHEN** 用户运行 check，随后运行 verify
- **THEN** check 可报告该内容范围覆盖率 100%，但这不处理另一 range 的责任
- **AND** verify 仍报告该未处理责任，不能凭覆盖百分比通过

### Requirement: Authority and write preconditions remain independent of cache

用户决定、理由、提交及必要版本依据 SHALL 保存于权威持久记录。file/command 方式的完整内容 SHALL 自持久化；git 方式的必要定位 SHALL 自持久化，而内容按上述约定从 Git 对象取得。SQLite SHALL 仅为可重建索引，不保存任何必要依据的唯一一份。系统 SHALL 拒绝并发更新和与调用方依据旧版本不符的写入，不自动合并、覆盖或换用新 hash 重试。人和 AI SHALL 使用相同约束。

#### Scenario: A stale writer cannot replace a newer record
- **GIVEN** 调用方读取了版本 V，写入前相关权威状态已变为 V2
- **WHEN** 调用方仍基于 V 请求提交
- **THEN** 请求被拒绝且旧记录不被覆盖
- **AND** OMD 不替调用方重新读取 V2 后偷偷重试

### Requirement: Ambiguous or deleted ranges require explicit coordinate commits

无法可靠定位时系统 SHALL 保留原范围和完整旧来源，报告旧坐标、差异和可用候选，不自动选择另一处同文。用户 SHALL 能通过当前范围 tip 的 `--id` 与显式 `--range` 提交选择。拆分 SHALL 不自动生成新身份或复制 link：用户可以保留一个连续范围，或在原跟踪保留一段并显式新建其他范围。正文全部删除但来源仍存在时，用户 SHALL 能提交 `p:p` 和理由，保留原 range/link 身份、记录完整当前来源并按普通变化处理下游；其覆盖贡献为零。空目标 MUST NOT 为非空来源范围提供合格 link 覆盖。缺失整个来源不能当作该空区间操作，clean/note 不能修复无法解释的坐标或断裂依据。

#### Scenario: Confirm a removed body as an empty range
- **GIVEN** 被跟踪段落已从仍存在的文件中删除
- **WHEN** 用户在原 range 当前 tip 上明确提交合法 `p:p` 与删除理由
- **THEN** 新版本范围为空，旧版本、身份、link 和历史保留，变化责任照常处理
- **AND** 不贡献覆盖，不把文件标成 tombstone，不自动清除下游责任

#### Scenario: A duplicate fragment is not chosen automatically
- **GIVEN** 原范围失去可靠匹配，当前来源另有相同片段
- **WHEN** 用户运行 verify
- **THEN** 报告定位问题与候选，不静默把跟踪改指该片段或保留有效确认

#### Scenario: A split does not copy relationships
- **WHEN** 用户为一段拆开的正文显式保留原 range 并新建第二个 range
- **THEN** 两者是独立跟踪，新 range 不自动继承原来的 link

### Requirement: Replacement targets a complete source version binding

`replace <commit-id> --source <引用> --expected <凭据>` SHALL 选择该 commit 引用的完整来源版本；没有此依据的元数据操作 SHALL 被拒绝。绑定按来源版本 ID 更新，结果 SHALL 列出共享该版本的受影响记录；不同版本即使内容 hash 相同也 MUST NOT 批量改绑。绑定修订 SHALL 与原始不可变输入分开持久化，不改变当前观察定义。`gc --content` SHALL 仅显式释放不再有保留记录要求本地副本的内容，并在释放 Git 替代的最后本地副本前重新核验其精确恢复依据；缺失、共享或保护需要不能被忽略。

#### Scenario: Shared version IDs differ from equal content hashes
- **GIVEN** O1/O2 引用同一来源版本 V，O3 引用另一个版本 W；V/W 完整内容相同
- **WHEN** 用户选择 O1 成功 replace 到匹配 Git 内容
- **THEN** V 的恢复 binding 改变并报告 O1/O2，W 和 O3 的 binding 不变
- **AND** 只要 W 仍需本地副本，显式内容 gc 也不能删除那份共享内容

### Requirement: Single updates publish through one authoritative state selection

系统 SHALL 使用固定文件上的 OS 独占锁及 `--expected` 观察凭据，核对相关存储发布代号、登记修订、节点 tip、来源 hash 与采集版本；冲突立即拒绝，不抢占锁或换用新依据重试。直接 init SHALL 校验目标未登记。文件写前重新核对读取 hash，command MUST NOT 因校验重跑；该检查不承诺阻止外部编辑器随后修改文件。

提交、登记、binding、inbound 修订及完整内容 SHALL 先作为不可变记录完整持久化，再通过同文件系统上持久化的 state 原子替换选择新状态。旧 state 引用的文件 MUST NOT 原地覆盖。state SHALL 保留尚未 gc 的已发布记录清单，reset 不删除清单中的历史。未发布文件 SHALL 不冒充 dangling commit。读取 SHALL 固定 state 并复核参与目录发布代号，不以混合快照宣布通过。平台不支持所承诺的锁定/替换/持久化能力时 SHALL 拒绝写入，不静默降级。

发布后响应丢失 SHALL 能通过 operation ID 与持久状态辨认结果，不自动重复命令；发布后同步结果不确定 SHALL 明确报告而非保证未生效。缓存失败 SHALL 不撤回已发布业务记录。未完成材料只诊断或由显式 gc 清理，MUST NOT 在启动时自动发布、续写或删除。

#### Scenario: An interrupted write does not publish an orphan
- **GIVEN** 新记录已写入，但 state 尚未切换时进程中断
- **WHEN** 用户重新打开存储
- **THEN** 原有效状态仍可读取，新文件不列为已发布 dangling 提交
- **AND** 不自动补发提交或重跑来源命令

#### Scenario: A lost response does not duplicate a successful update
- **GIVEN** 新 state 已持久发布，但响应未送达
- **WHEN** 用户按 operation ID 查询结果
- **THEN** 能辨认已发布结果，不自动生成第二条同操作提交

#### Scenario: A failed final synchronization is not a promise of rollback
- **WHEN** state 已切换但最终持久化确认失败
- **THEN** 输出结果不确定及 operation ID，不能宣称所有状态均未改变

#### Scenario: A reader detects a changed participant
- **WHEN** 读取过程中某个参与存储的发布代号发生变化
- **THEN** 返回冲突或不完整诊断，不把混合版本结果当作通过
