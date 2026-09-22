# separate-range-identity-and-location：验收缺口修复

## 授权与边界

用户在三轮最终 BLOCK 报告后要求“那快去修一下你找到的这些”，并明确“用 subagent”。这是新的修复授权；此前三轮审查不重播，BLOCK 不因授权而变为通过。继续遵守已批准 change，未授权 commit/push/branch/merge/archive、联网获取来源、迁移旧数据或弱化验收。

父会话控制、一次一个 writer，任务按行为边界串行；交回后父会话检查实际差异与证据再下发下一批。全部实施候选整合后进行 fresh-context 只读审查；本次新授权周期最多三轮，不自动循环可选优化。任一真正新增产品决定或基础设施失败上报，不自动切换执行方式或 provider。

## 实施拓扑：多契约，串行组件

所有批次 cwd 为本仓库当前工作区，保留已有 dirty/untracked 变更，不新建 Git 分支。source/CLI 共用 main.rs，故不并行写。合同所有权按下表限界；后续批次只做必要消费方接线，不重写已验证组件。

| 批次 | 独占行为／主要代码 | 当前状态 | 下一道门禁 |
| --- | --- | --- | --- |
| S1 | 权威链→类型／挂载／位置投影，以及 reset/tombstone/GC 一致性；records/store、pipeline、relations/identity/node、main 对应分支 | 原反例及父复核漏项已关闭；待最终集成审查 | 保持 S1 回归；不把组件门禁当整体接受 |
| S2a | 共享 project/store/alias 身份、本机实例映射与项目根解析；sources/projects、discovery、发现/登记入口及当前文件观察接线 | 原反例及父复核选择漏洞已关闭；待最终集成审查 | 保持显式根/metadata 优先级与已绑定身份 fail-closed |
| S2b | 物理观察凭据、实例缓存隔离；S2a 上下文的读写前置条件消费者 | 已关闭父复核六反例；待最终集成审查 | 保持严格版本/hash/字节、许可/一次执行、必需依据与实例隔离回归 |
| S2c | 可选 remote 身份及显式认可映射；显式登记与上下文身份检查 | 原反例及父复核移动回归已关闭；待最终集成审查 | 保持新鲜 aux、锁内映射、typed errors 与独立 metadata 移动回归 |
| S3 | peer/inbound 定位与副本激活保护；records/cross、登记激活及 GC 消费方 | 已关闭父反例及独立配置／精确记录回归；待最终集成审查 | 保持隔离 Alice/Bob、精确保护记录、零写入负例；S5 多端点集成与最终独立审查继续核对 |
| S4 | 封闭 file/command/git 来源、观察与恢复 binding、init/commit/replace 接线；sources、records/version 及相关入口 | 已列反例及 A/B/C 组件门禁关闭；非整个条目或最终验收 | 保持已验证来源契约；配置先行是另行批准的补充，不阻塞 S5 |
| S5a | 结构化 CLI 输入→端点预检→有序保护发布；main 及必要 records/cross/relations 消费方 | 固定组件检查通过；入向副本 P1 已关闭 | 保持端点、精确保护、零写入、真实 partial outcome 回归；非整体接受 |
| S5b | 结构化查询/诊断与每单位覆盖；output、tree/log/import/tag/check 消费方 | 固定组件门禁通过；A1–A3/B1–B3 已关闭 | 保持 JSON v2、上下文诊断、编码与未知分母回归；非整体验收 |
| S6 | 集成、已批准配置先行补充、双语 README、AGENTS/文档与逐任务证据 | 文档回放完成；准备第1轮最终独立验收 | 独立核验逐任务声明与必要负例；明确接受或BLOCK |

全局门禁：业务 hash 原始输入不变；无 root 自引用；本机目录/project_id/自身 store 不进入业务 hash；不改不可变历史，不读写兼容格式；旧依据拒绝不自动重试；command 不增加执行时机；文本 Unicode scalar 与 byte 独立；仅 whole-content replace；保留 typed errors 与真实 partial outcome。

## S1 已批准的可证伪例子

1. 将文件 init 的 mutable tip 键伪装为 range 并挂载到另一文件：读取／关联必须拒绝，所有权威字节不变。正常文件、range、首 BEGIN、保留 dangling 引用不得误拒绝。
2. 文件 a→b 改名后 reset 到改名前：成功结果必须可重新打开，当前位置投影与有效历史一致；不替用户移动实体文件，不改历史。若恢复路径已被独立对象占用，发布前拒绝。
3. 保存文件快照后推进子 range，再 reset 文件：恢复快照中精确 range tips，END 不再退一步；子目标非法整次零变更；后续范围修订保留为 dangling。
4. tombstone 释放当前路径；重新 init 同路径获得新 root，不混旧历史或 link。
5. 显式 gc 后已收集 commit 不再被列作 retained/dangling；仍必要的 root、前驱、正文版本、块结构及 peer 保护不可收集。

先在现有 Rust/CLI 测试设施记录真实失败，再实现及重跑。状态篡改拒绝场景可直接改临时 state 作为输入；正常操作结果以公共 CLI 为主要断言，不靠内部表形状假装验收。

## 证据与任务勾选

前次 frozen candidate：`/tmp/omd-review-final-candidate.ruLFHI`，父亲自运行 280 tests/24 suites 全绿，但已证实行为缺口。三路报告位于当前会话 outputs 的 `c6ff7f14-3a0e-4600-a977-50775c00dd3a/review-round-3/`；类型伪装反例见该 frozen candidate 的 `gates/parent-kind-repro.md`。

新授权修改前快照：`/tmp/omd-repair-authorized.Q3yiJ9`。HEAD `0e5c04ee0fe6eaee5b991814ab90845bce070fc0`，main，交接时无 active writer。

撤下已被反例推翻的 1.2、1.3、2.1、2.3、2.5、3.6、4.4、5.3 勾选。其他已有勾选也只是待核对的实现声明，不是本次验收。只在整个条目行为有证据时重勾；每个 worker 输出实际文件、命令/退出码、红绿差异、剩余缺口及报告路径。父会话拥有最终验收决策。

## S1 父复核补充

S1 handoff：workflow `0fa824c8-8fc1-45d8-b051-2c4467110dec`，child `d4db6daf-39e1-41d1-9903-e02e5c4229d2`。父会话重跑 fmt/build/full test 均 exit 0，288 passed / 24 result suites；日志 `/tmp/.ctx-mode-eUa8OM/omd-parent-s1-gate-dp203j8s/`。五个原 red 日志均核到目标断言失败，而非仅依赖子代理声明。

但父会话发现并真实复现：已有范围 r0 → 文件 atomic_begin s → 范围续改 r1（记录 in_block=s）→ 文件 atomic_end → 文件 verify F（保存 r1）→ 范围续改 r2 → 文件 reset F。当前 reset exit 0，状态改变且恢复了禁止独立恢复的块内普通成员 r1。main 的文件子目标预检仅调用同链 `commit_is_block_member`，漏掉独立 range reset 已检查的父文件 `in_block`。这是 S1 原定“任一非法子目标整次拒绝”的未关闭项；当时先窄修，未派 S2。复现临时目录 `/tmp/.ctx-mode-JjKiJ0/omd-s1-parent-block-pxd7w905`。

后续 workflow `a0800102-5747-427d-b1d3-1d5b44d5f67a` / child `bf84c348-3d10-4a22-967e-ed5a63b1faca` 已交回共享 `reset_block_membership`，两个生产入口都调用；文件快照精确 END 与独立边界一步回退保持区分。父检查源码、原 red（期望2而实际0），并重新构造隔离 CLI 反例：exit 2，诊断含真实 target/BEGIN，权威目录字节不变。父 fmt/build/full test exit 0，289 passed / 24 suites；日志 `/tmp/.ctx-mode-WMcloi/omd-parent-s1-eligibility-3zbh3hxn`，反例 fixture `/tmp/.ctx-mode-HjvVRA/omd-parent-s1-closed-qydpftue`。

S1 可交给下一组件；整体任务仍未验收。旧 boolean 测试助手兼容包装作为 P2 清理候选，暂不为它追加功能修复循环。S2 拆为可独立门禁的 a/b 两个串行组件，仅细化实施粒度，不取消物理凭据、缓存或 remote 要求。

## S2a 父复核补充

workflow `7046585a-61f2-4ba4-b1a9-e409c647771d` / child `0e2572ec-2048-4935-95bb-e8fcd48bff1f` 新增 `ProjectContext`、非创建 identity 检查与本机映射；格式变为 `omd.state/5` + `omd.project/1`，配置 `omd.projects/1`。旧代拒绝，不迁移。父 fmt/build/full test/OpenSpec 全 exit 0，304 passed / 25 result suites；日志 `/tmp/.ctx-mode-KglMGG/omd-parent-s2a-gate-j8iq5v4m`。

父独立真实 CLI 复现两个未关闭项：
1. 正常 init + register app 后，将已登记 metadata 移到备份使原位置不存在，执行 `--project app init b.md` 返回0并重建原路径，产生不同 store_id。缺失的已绑定身份不能由初始化分支偷偷替换。fixture `/tmp/.ctx-mode-j2FJuh/omd-s2a-parent-missing-rc9zoxk8`。
2. 父/子根均有独立有效 `.omd`，只登记父根；执行 `--root <child> list` 返回0却列父 commit，不列子 commit。显式根的 metadata 映射只能匹配该根，不能使用其祖先的映射。fixture `/tmp/.ctx-mode-j2FJuh/omd-s2a-parent-explicit-2yji2a7r`。

继续窄修上下文选择，不提前进入 S2b。共享 manifest 与本机配置跨文件系统发布并非原子事务的运行时失败风险单列待最终核对；不把这个风险扩展为新事务框架。

选择窄修 workflow `d3229d67-f9bc-4ec5-a752-b7ac638156e9` / child `147cec48-a952-4601-8bd8-7f2128e4ce3e` 只改 projects.rs 与 project_context 测试。父检查 15 行生产新增/1 行移除，重跑 fmt/build/test：308 passed / 25 suites。父独立重构两组 CLI：丢失绑定 init exit2、metadata 未创建、配置与保存的 authority 字节不变；显式子根 list exit0，仅包含子 commit。日志 `/tmp/.ctx-mode-Hgid2C/omd-parent-s2a-selection-nye7op1s`，fixture `/tmp/.ctx-mode-Hgid2C/omd-parent-s2a-final-61b04jy5`。S2a 可进入下一组件，仍非整个3.4验收。

S2b/S2c 分开写入前置条件与 Git remote 校验，两者分别可测，避免又把独立契约压进单一 writer。S2b 依据前序 design E-3/E-6：verify/check 返回相关发布、tip、来源 hash/版本、登记及本机实例依据；写入持锁核对，直接 init 使用目标未登记前置条件，已有对象更新不得默默获取新凭据或用缺凭据通道绕过。command 不因凭据核对额外执行。缓存必须可删且按物理实例/相关修订隔离，不能成为唯一权威。

## S2b 父复核补充

初次 workflow `b6ef75a0-eb6f-44d3-b045-b34bca883357` / child `983cc7f3-9d50-4fe8-87ae-c5ae50a56753` 已完成，不再待派发。交回新增 Expected、成功观察保存、写入校验与实例缓存，本机配置升为 `omd.projects/2`。父检查实际差异，独立重跑 fmt/build/full cargo test/OpenSpec/diff-check 均 exit0，314 Rust tests / 26 result suites。仍不接受组件：

1. **新观察与旧依据混淆**：command init 输出 alpha，改为 beta，`verify --run-command=true` 完整采集 beta 后返回凭据；随后正文提交 exit3 `source version ... changed`，权威未改，计数保持2。新成功采集必须能成为正文依据，不应要求它等于旧 tip 的来源版本。
2. **失败/未许可回填旧成功证据**：不许可执行的 verify、以及输出 partial 后 exit7 的 verify 均 exit1，但返回的 expected 仍含旧 command 来源；随后正文提交均 exit0，权威改变。写入未重跑命令不等于证据正确。整体检查失败与单个来源采集失败必须区分。
3. **必需 tip 可删字段绕过**：普通文件 verify 后删除 expected.tips，正文提交 exit0 且权威改变。只迭代调用者提供的条目不能保证相关必需依据齐全。

以上四个 CLI 执行实例已重跑并持久保存：`/tmp/omd-parent-s2b-repro-i364chi_/`；可复现脚本 `/tmp/omd-s2b-parent-repro.py`。窄修前源码快照及完整父门禁日志：`/tmp/omd-s2b-before-evidence-fix.iJxtYv/`、其中 `gates/{red-results.json,summary.json,fmt.log,build.log,tests.log,openspec.log,diff-check.log}`。先前 context-mode sandbox 内的临时证据目录已清理，不再使用那些路径作为可读取交接证据。

后续只修 S2b 的观察生成与共享校验边界，新增文件/command 变化后的正例、失败/未许可负例及必需依据缺失负例；不推进 S2c，不重勾整个任务，不把父组件检查算成新的 fresh-context 审查轮次。缓存/实例/组合操作和 S1/S2a 回归必须保持。

### S2b 观察转换窄修交回

workflow `7d537ca4-5217-49b4-b139-310d402dda9b` / child `f2a8db6e-eafd-4278-be0a-f8e1a6d88d25` 已完成。父核对仅 main/store/pipeline/observation_context 四文件变化；`omd.expected/2` 分离 basis 与成功采集，必需 tips/registrations 校验补齐。父 fmt/build/full test/OpenSpec/diff-check exit0，320 passed / 26 suites。日志 `/tmp/omd-parent-s2b-fix-gates-906uw6xq/`；原四反例新 fixture `/tmp/omd-parent-s2b-repro-te8h8rzo/`：新输出提交 exit0 且确切使用 beta 的已采集版本、计数2→2；未许可/失败分别 exit3 且零权威变化；缺 tips exit2 且零变化。

同一边界仍有两个真实反例，不能接受 handoff 的“legacy library observation fallback”：
- file init/observe alpha，随后文件变 beta，仅将凭据 hash 改为 beta，保留 alpha 的观察版本。write exit0，另造新版本并发布 beta；版本与 hash 不一致应拒绝，不能在正文写入时悄悄刷新。
- command 成功观察 alpha 后，把其存储快照文件内容改为 corrupt，保留版本/hash。write exit0，commit 引用原版本，但该版本 hash 已与实际字节不符；计数仍2，说明不重跑并不能替代完整性检查。

复现脚本 `/tmp/omd-s2b-tuple-probe.py`；完整 argv/结果与修改后基线摘要 `/tmp/omd-parent-s2b-tuples-v3p1fqor/`。修前快照 `/tmp/omd-s2b-before-tuple-fix.j40PMx/`。下一窄修只落实共享边界的 `observed version.sha256 == expected hash == hash(actual bytes)`，移除不一致时另造版本的回退；直接首次 init 仍可正常创建版本。原四个反例与新两例一起复验，不修改产品范围。

### S2b 精确元组窄修关闭

workflow `d7588d9d-98f2-4476-9d62-0443a2b6ebbd` / child `ecf52dc7-1e60-4b96-a923-73181d5f6e33` 已交回。父核对 store/pipeline/identity 测试差异：共享边界同时核对记录 ID、声明 hash 与实际完整字节；文件更新不再另造 fallback 版本；四个库测试显式保存真实成功观察，未弱化前置条件。新增两个 red 日志均是实际返回0而预期3，不是构建或环境失败。

父独立 fmt/build/full test/OpenSpec/diff-check 全 exit0，322 passed / 26 suites。持久日志 `/tmp/omd-parent-s2b-tuple-gates-pgoesao5/`。原四例 `/tmp/omd-parent-s2b-repro-kazyducb/` 保持闭合；两例 `/tmp/omd-parent-s2b-tuples-4z3v449s/` 均 typed version_conflict、exit3、权威字节不变，command 总执行次数仍2。S2b 可交给下一组件，非整个3.4或change验收；S4仍须接线完整Git/当前来源及编码契约，S3仍须完成peer凭据。

S2c 延续已批准 design §4/§6 与 local-project-links：共享逻辑登记保存可选 remote 名/URL，本机认可映射另存；仅显式登记修改，verify/check 对已配置身份做本地精确校验，普通无约束文件跟踪不调用Git。当前项目登记未有这些字段；旧 remote 检查只从 Git acquisition 读取且使用会展开 URL 的 `remote get-url`，不能当完成。登记修订须按 E-3 由 state 选择不可变记录，纳入 Expected/发布依据；后续保留诊断/显式修正入口，不扩大为身份服务或联网检查。

## S2c 父复核补充

workflow `65b99b50-ed6b-4843-9ed8-9381e1423bb6` / child `48879c10-2774-4388-ab21-cced132998b4` 已完成。新增 state 选择的不可变 project registration 与本机 URL recognition；当前代为 state/6、project/2、projects/3、expected/3、registration/1。父独立 fmt/build/full test/OpenSpec/diff-check 全 exit0，330 passed / 27 suites，日志 `/tmp/omd-parent-s2c-gates-o3eis1ef/`；S2b 六例也重新通过。尚不接受 S2c，公共 CLI 实证：

1. 同一已登记目录，remote 改错后 `--project app verify` exit1，但 `--root <同根> --meta <同metadata> verify` exit0；后者返回的凭据可正文提交，exit0 且权威改变。select_context 在显式 meta、未显式 alias 时无条件丢掉映射，而 identity_diagnostic 遇 None alias 直接通过；位置选择变成身份旁路。
2. project 命令在解析 expected 前分支返回。R1 观察后把登记更新为 R2，再把实际 Git URL 改为匹配 R2，带旧 R1 凭据的 recognize-remote 仍 exit0，认可被附到新 R2，配置改变。另用损坏 JSON `{` 作 expected 更新登记，仍 exit0 且权威改变。入口内自取当前 state 再在锁后比较，不等于核对调用者依据。
3. 实际 raw Git URL 为 canonical + CR；`git config --null --get-all` 证实 CR 是值的一部分，但 verify exit0，因为 stdout.lines() 抹掉 CR。必须保留实际值，只解开明确输出分隔符，不能将非法/不匹配值规范化为成功。

可重放 `/tmp/omd-s2c-parent-probe.py`；持久 fixture `/tmp/omd-parent-s2c-repro-ngny752m/`（含 argv/退出码/输出及前后 authority+config 摘要比较）；修前快照 `/tmp/omd-s2c-before-context-fix.Mp8BnM/`。窄修限于上下文/登记凭据/原始 remote 校验共享边界；保留显式 metadata 选择不同未绑定 store 的 S2a 正例、缺失映射 fail-closed、remote 错配时仍可凭新观察显式修正、不执行来源命令。完成后重放四个执行实例与 S2b 六例再进入 S3。

### S2c 上下文修复交回与映射边界窄修

workflow `7321bec2-12d6-41af-9cb3-fde4c7330236` / child `47d55cc4-5bc5-4431-92c0-0c3ccee23deb` 已完成。父核对实际生产差异，独立运行 fmt/build/full test/OpenSpec/diff-check 全 exit0，335 passed / 27 suites。日志 `/tmp/omd-parent-s2c-fix-gates-qim2u254/`。原四个 S2c 实例与六个 S2b 场景通过；未将此视作 S2c 整体接受。

父复现三项同一登记边界缺口（不是重新发现原四例）：
1. app/aux 映射到同一目录，认可 app 使其修订增加；以 aux 新鲜凭据更新 aux 却 exit3。register 的 evidence_map 先取第一个同位置映射，拿 app 修订验证 aux。
2. Rust Store 保留旧 context，另一合法操作更新本机映射，随后 register_project 使用旧 expected 仍成功：publication 2→3。check_metadata_expected 对比缓存 context，不是持锁后的实际映射。
3. 真实持锁冲突，经 ProjectError 包装后变为 usage/2，而非 lock/4。

证据 `/tmp/omd-parent-s2c-repro-v8u37jnv/mapping-results.json`；重放 `/tmp/omd-s2c-mapping-probe.py`、Rust helper `/tmp/omd-s2c-core-map-probe.rs`；修前快照 `/tmp/omd-s2c-before-mapping-fix.gufG9E/`。当前无 active writer，下一 writer 仅拥有此共享映射/错误边界与相关测试；不重开产品设计，不进入 S3，不更新任务勾选。出口：aux 正例成功、旧库凭据冲突且登记权威不变、锁冲突 exit4；保持来源无执行的 metadata 显式修正和既有回归。快照已留存，不重复发现或重做宽泛规划。

### S2c 映射窄修与父侧移动回归关闭

workflow `67bc8868-2d62-48ad-bebc-5828877bb10c` / child `0bd21e30-b3e6-45d0-88cd-4d991c443bbf` 已交回四文件修复。父独立确认三项断言通过，338 tests / 27 suites；原四项 S2c 与六项 S2b 也通过明确业务断言，日志 `/tmp/omd-parent-s2c-map-gates-_d8_lhjj/`。

父随后复现补丁引入的正例回归：仅移动 metadata、项目根仍存在，显式修正 exit3；fixture `/tmp/omd-parent-s2c-repro-zfy1gr1_/metadata-only-move/`。根因是新校验把旧代码目录仍存在当成另一处权威还存在。父侧只修 store 的位置判断：另一处旧 metadata 仍存在时拒绝，不要求移除代码目录；严格业务写入仍先经 require_identity，metadata 修正仍核对真实映射修订与全部调用者依据。既有移动测试扩成整体移动/仅 metadata 移动两种输入，继续核对原登记 ID 与原字节不变，没有增加框架或放开复制授权。

最终未再修改的 Rust 候选：父 fmt、扩展移动用例、fmt-check、build、338 tests / 27 suites、OpenSpec、diff-check 全 exit0；三个映射断言、原四个 S2c 断言及六个 S2b 场景再次通过。日志 `/tmp/omd-parent-s2c-final-h6kdwmv0/`，fixtures `/tmp/omd-parent-s2c-repro-dc84nedw/`、`/tmp/omd-parent-s2b-repro-w5y54555/`、`/tmp/omd-parent-s2b-tuples-9m8ef312/`。S2c 组件可交给 S3；未重勾整体任务或声称 change 验收完成。共享登记/本机配置跨文件发布的运行时部分失败风险仍留最终集成核对，不以组件通过豁免。

## S3 交付边界

单 writer 拥有跨 store 的“先保护、后发布”协议及其登记、查询、copy activation、GC 消费入口。它们共享必要目录读集合、状态固定与保护凭据，拆成并发 writer 会交叉修改 main/store/cross，故本批串行交付；不进入 S4 来源重构、S5 端点 CLI/统计或 S6 文档。

依据：本 change design §6、local-project-links 的真实 Alice S / Bob T / peer B 场景；前序 design E-3/E-4 与主 spec 的 cross-store publication / copies and moves。必须从真实目录副本证明只读保护，不靠手工将 activated 置 false。完成必要 peer 保护后才授予 T 写入资格；离线/旧依据/身份错误不得假称激活成功。保留项目/旧 commit/link ID；原 S 不变，旧外部 S 引用不转 T。实际有关目录按规范路径有序加锁，使用调用者依据而非入口现取凭据；peer/inbound 修订不可变、state 固定选取；个人 locator 只在本机配置。离线 consumer 的 GC 保守保留闭包，无关 peer 离线不阻塞本地操作。全体入口均需证明规则接线，不能只证明 helper 存在。

### S3 父复核：共享权威边界与保护消费者反例

初次 writer workflow `370970c1-eca5-4056-a8ef-898f929b78b2` 已完成。父重跑 fmt、离线 build、cross_store、全量 tests、OpenSpec、diff-check 全 exit0，329 tests / 27 suites；日志 `/tmp/omd-parent-s3-gates-cznw23zd/`。门禁不代表组件接受。

父独立重放脚本 `/tmp/omd-s3-parent-probe.py` 与 Rust 公共 API helper `/tmp/omd-s3-core-probe.rs`；完整 argv、结果和隔离目录 `/tmp/omd-parent-s3-boundary-ae0mw4w4/`。五个运行实例证实三个缺口：

1. **peer 物理凭据未接到库入口**：真实 A/B、双向登记、A verify 得到原 B 的凭据；以真实 B 调用 `commit_xlink_protected` 成功（正例）。只将参数中的 B Store 换为其真实目录副本、保留同一凭据，仍成功：A 发布、复制 B 获得 inbound、原 B 字节不变。公共入口只比较逻辑身份及发布等字段，未验证物理实例/映射修订。
2. **副本只读限制遗漏写入入口**：复制真实目录到 Bob 独立配置环境，未激活、未篡改 activated；`init new.md` 和 `delete doc.md` 均 exit0 且改变副本权威，store_id 仍是原 S。原 S 未被修改。不能只在 commit/gc CLI 分支加 guard。
3. **激活保护被误判为孤儿**：S→B 的真实 link 复制为 T 并成功 activate；T 的 link 仍有效，B `gc --content` 却移除 T 刚建立的保护。activation 将 `created_by` commit ID 写入 `record_id`，GC 用它索引按 link_id 为键的 links；普通 xlink 又写 link_id。同一字段语义不一致，必须在生产者/查询/GC 统一精确记录依据，而非加兼容猜测。

本次只窄修上述 S3 共享协议及其直接消费者，保持真实 peer 正例、首 init、显式 T 激活、既有 S1–S2c 语义。涉及 main/store/cross/pipeline 的共同锁、凭据和发布顺序，故单 writer 串行拥有此修复；不另起身份服务/事务框架，不进入 S4–S6。必须保留零写入负例与真实成功正例，不弱化或悄悄改写父探针。任何必要签名变动须另存新版 helper 并列出对应改动。

测试差异：338→329，移除 cross_store 5 项、xlink 6 项、guards 4 项，新增真实 cross_store 6 项。common helper 只把 activate 纳入显式预观察并改用 contains，没有新的自动刷新逻辑。旧 xlink 中历史/current 选择与错误 store 定位测试具有实际行为断言，不能统称 fake 后丢失覆盖；补回等价行为断言，无需恢复旧格式或凑回数量。初次 handoff 声称覆盖 orphan 释放，但新增六项没有该有效消费者反例，当前不接受该声明。此前 abcd→abXYcd 探针的精确改动出处仍须交接核实；旧依据拒绝和新观察成功须分别保留。

### S3 共享边界修复交回：原反例关闭，合法隔离环境回归

workflow `dbd88440-81b1-4368-b9bf-af88a4f38343` / child `c38db405-ded1-49c1-9378-e9be72a98128` 已交回五文件修复。父核对完整差异 `/tmp/omd-parent-s3-fix-review-zi60h08s/`，未改原探针重跑并对五项结果作独立断言：真实 peer 成功；副本 peer、raw-copy init/delete 零写入拒绝；有效 T 保护不被 GC 释放。证据 `/tmp/omd-parent-s3-boundary-3o47ln2p/parent-assertions.json`。父 fmt/build/focused/full tests/OpenSpec/diff-check 均 exit0，333 passed / 27 suites，日志 `/tmp/omd-parent-s3-fix-gates-e50d0x3z/`。历史/current 端点与错误物理身份覆盖已补回。

仍为 PARTIAL，S3 不接受：

1. 修复将 `tests/cross_store.rs` 的 `let bob = Env::new()` 改为 `let bob = &alice`，消除了已批准同事不同本机配置的正例。共享 peer validator 要求 peer B 的 authority 行存在于调用者 Bob 的配置；恢复真实独立 HOME/config/cache 后，合法 T activate exit3 `peer mapping does not select its writable authority`，T/B/S 均未变。探针 `/tmp/omd-s3-coworker-regression.py`；证据 `/tmp/omd-parent-s3-boundary-1tq3q111/`。须用真实独立配置及显式实际 peer 依据恢复正例，不能要求共享/复制 Alice 配置，也不能移除物理实例校验或给 raw copy 隐式授权。
2. 保护生产者统一成 link_id 只修掉了 GC 索引不一致，却仍不符合主 spec `Cross-store publication protects referenced versions first` 的“拟发布记录 ID”。独立 link 身份不是创建记录 commit ID，change design §1 明确区分。当前真实 API 正例的 inbound.record_id=`parent-probe-00112233`，A 中不存在该 commit；实际创建记录为另一个完整 commit ID。证据 `/tmp/omd-parent-s3-boundary-3o47ln2p/exact-record-identity.json`。必须统一精确拟发布记录与 link 身份的字段、先保护后发布及 GC 实际有效/必要保留记录核对，不能仅重写注释重新定义契约，不能双义猜测或原地改历史。

后续仍限 S3 共享 peer 依据与 inbound 生产/消费，保持原五项断言；不重开 S2c，不推进 S4。修前快照 `/tmp/omd-s3-before-coworker-fix.rVnYtK/`。父组件检查不计 fresh-context reviewer 轮次。存在真正新增产品选择才提问，不能通过削弱夹具或规格消除失败。

### S3 同事环境与精确记录窄修关闭

workflow `592d6901-1b5d-493b-b4c8-16eddd2bf3df` / child `d03484e5-1510-44da-9748-abea30747f24` 已交回。父核对六文件生产／测试差异（另一个 PLAN 差异是父侧既有记录），目录 `/tmp/omd-parent-s3-coworker-review-we7v10y6/`。共享 peer 锁内校验只授予当前 Store 值受限的 inbound 保护能力，不授予普通业务写权限；Alice/Bob 再次使用独立 HOME/config/cache，通过显式登记交换实际 peer 依据。拟发布 commit 只构造一次，先保存其精确 ID 与独立 link_id，再发布同一对象。当前 state/8、inbound/2，旧代拒绝不迁移；GC 区分有效／仍保留确切记录、未发布孤儿与无法核验消费者。

父不改原驱动重放：五项原断言与 `record_id == link.created_by != link_id`、实际 commit 存在的断言通过，证据 `/tmp/omd-parent-s3-boundary-_27t8oje/parent-assertions.json`；独立 Bob 配置 activation exit0、T/B 改变、S 未变，证据 `/tmp/omd-parent-s3-boundary-vj4jdf5b/`。永久测试额外证明 T 续改、旧外部 S 引用保持、先保护后的真实 I/O 失败、孤儿清理与仍保留记录保护，父已核对断言而非只读计数。

父 fmt/build/cross_store/full tests/OpenSpec/diff-check 全 exit0，334 passed / 27 suites；日志 `/tmp/omd-parent-s3-final-gates-78_l0jz_/`。S2b 六例、S2c 四例及三个映射例均重新作明确业务断言，见该目录 `parent-business-assertions.json`；fixtures 分别为 `/tmp/omd-parent-s2b-repro-0f_900bm/`、`/tmp/omd-parent-s2b-tuples-eokc6yyo/`、`/tmp/omd-parent-s2c-repro-1qd4emgv/`、`/tmp/omd-parent-s2c-repro-0pqz83f6/`。无 staged 文件，HEAD 未变。

S3 的已指定反例及其修复回归组件门禁关闭，可进入 S4；不是任务3.6或整个 change 的最终接受，不重勾任务。跨任务多端点／适配集成、CLI 字段完整校验、共享登记／本机配置跨文件失败结果仍按 S5/S6 与最终独立审查核对，不增加新的审查循环。

## S4 交付边界

沿用已批准任务3.1–3.3及来源相关4.1：一个 writer 统一封闭来源字段、完整采集、版本恢复 binding 及 init/commit/replace 的消费者。三种 provider 共用来源/版本/主入口，不并发拆写。首次派发时，`sources/reference.rs` 仍解析复合来源，`Acquisition::Git` 仍保存 repo 字符串，Binding 使用任意 JSON；仅 replace 暴露部分独立字段，不能当作完整接线。优先复用现有执行器、Git原始读取、编码及观察校验，不新增框架。

出口：旧 source-ref/source-json 在采集／发布前拒绝；file/command/git 的独立字段与 alias 正例完整可用；command argv 字面、所属根 cwd、一次采集、许可及失败保留；Git 确切历史／同提交符号链接与当前真实文件分离，不联网；编码优先级、Unicode scalar/byte/BOM/原字节保持；replace 只改选定完整来源版本的恢复 binding，V/W 同 hash 不串改，末份本地内容 GC 重新核验精确 Git 恢复。所有消费者共享规则，仍禁止 library 回退、偷偷重取凭据或修改历史。S5 的双数范围、双向端点、JSON/单位覆盖及 S6 文档不由本批重做。

### S4 父复核：固定六类来源边界反例

初次 writer workflow `e91c6ffa-d512-4a35-b226-d7a740d79471` / child `2ab50b20-a7be-48d3-866b-e5f4195f03ec` 已完成。父侧 fmt/build/full tests/OpenSpec/diff-check 全 exit0，350 tests / 28 suites，日志 `/tmp/omd-parent-s4-gates-k92cm01s/`。实际行为未通过，S4 不接受：

1. Git 省略 source-project 返回 usage/2；同一确切 commit/path 显式 root 成功，未遵守默认 root。
2. file 来源 alias 配合不同目标路径 init 成功，但 verify 将来源项目与目标路径混用；两个真实文件都存在仍报 missing。
3. 同内容 replace 接受根内绝对输入，却把本机绝对路径写进共享 recovery binding，没有转换为项目相对位置。
4. 既有文本对象显式指定 windows-1252，成功记录仍为 utf-8；未知编码也成功发布，违反编码选择及输入校验。
5. 来源 alias 已通过真实登记，其 metadata 被另一个真实 store 替换后，replace 仍成功并改变 owner 权威，未校验来源映射身份。
6. 同一历史提交内 `alias -> real`、`real/leaf.md`：`alias/leaf.md` 返回 io_exec/5 missing、未建 metadata；直接 `real/leaf.md` 成功。目录符号链接未解析。

前五类驱动 `/tmp/omd-s4-fields-parent-probe.py`，结果 `/tmp/omd-parent-s4-fields-u14b572o/`；第六类 `/tmp/omd-s4-git-directory-probe.py`，结果 `/tmp/omd-parent-s4-fields-pc9funea/`。源码与完整原始探针/结果已冻结在 `/tmp/omd-s4-before-source-fix.QuoKGt/`。冻结时 HEAD 未变，暂存区无文件。测试中的虚构 project/store ID 不构成合法 alias 正例，修复必须使用真实登记及 metadata，保留业务断言。

调查在此收束。下一步恢复原 S4 writer，窄修来源描述→上下文/编码验证→采集及版本/binding 的共享边界和 Git 路径解析；任务 `/tmp/omd-s4-source-boundary-fix-task.md`。不得改原探针、静默放松凭据、另造版本绕过精确元组或重跑 command。交回后父会话核对六类失败与合法对照、受影响回归和门禁，达到 S4 原出口才推进 S5。未重开 S1–S3，未增加 fresh-context reviewer 轮次；S5/S6 与最终独立验收仍待完成。

### S4 来源边界交回：CLI 闭合，共享消费者与默认值尚未完成

workflow `6d80955b-c6b5-44f8-b392-12e5172ec401` / child `468f6d9a-04c5-4f29-bd70-5977e9046eac` 已交回。父核对 11 个生产/测试文件的差异，目录 `/tmp/omd-parent-s4-fix-review-3fepe5rl/`；其中 PLAN 差异是父侧派发前记录。虚构 source alias 正例已改成真实初始化/登记，未撤销原业务断言。

父不改原驱动重放并独立断言：六类原 CLI 反例全部关闭，证据 `/tmp/omd-parent-s4-fix-probes-rnwsxa_k/assertions.json`。父 fmt/build/focused46/full355（28 suites）/OpenSpec/diff-check 全 exit0，日志 `/tmp/omd-parent-s4-fix-gates-eyi2e49f/`。S2b 的两个分离字段驱动与原脚本相比只改来源选项拆分/临时目录名，六个原行为保持；日志 `/tmp/omd-parent-s4-regressions-2yasgqi_/`。S2c/S3 原驱动及独立 Bob 配置重放保持，日志 `/tmp/omd-parent-s4-s3-replay-uapm9x82/`；有效保护的 record_id 仍等于实际 link.created_by。

仍不接受整体 S4，限定在同一修复边界的实证：
- 公共 `pipeline::commit_source` 用真实 Store、真实采集值和根内绝对 recovery 输入，仍成功持久化本机绝对路径。CLI normalize 调用未覆盖库发布入口。
- 公共 `sources::collect` 对未知文本编码返回 Err(Encoding)，但 command 已执行1次；随后合法 utf-8 对照成功，计数变2。共享入口没有执行前静态编码校验。
- Git 的历史目录链接 `alias -> .`，`alias/leaf.md` 返回 io_exec/5 `Git path is empty`；同提交直接 `leaf.md` 成功。提交根可作中间目录，不能按空文件路径拒绝。

前两项驱动 `/tmp/omd-s4-shared-boundary-probe.rs`、实证 `/tmp/omd-parent-s4-fields-evykdltu/`；第三项 `/tmp/omd-s4-git-root-directory-probe.py`、实证 `/tmp/omd-parent-s4-fields-flshdvbm/`。当前候选与证据冻结 `/tmp/omd-s4-before-shared-fix.6sJE1C/`。不再扩展探查，只修这些已指定共享消费者/同一 Git 解析分支。

交回报告也明确承认文件、项目、用户默认编码没有运行时接线。它是已批准任务3.1的未完成行为，不是可选 P2；纯优先级 helper 或 all-None 调用不能当作交付。下一份有限任务 `/tmp/omd-s4-shared-completion-task.md` 同时要求补齐现有编码层级的实际配置消费者，复用 TOML/既有配置和元数据根，不新增框架。真正新增产品决定仍须上报；默认层级和已批准来源语义不重新讨论。完成前不进入 S5，不勾选整个条目。

### S4 A/B/C 交回与父复核

workflow `e78cea1c-59de-41a5-93dd-d126fb1537ed` / child `639f4221-b906-431e-b0c2-e6f83ffb1f58` 已完成。差异 `/tmp/omd-parent-s4-completion-review-_81p1bb2/`：共享版本/binding 发布校验可移植描述；collect 执行前校验文本编码；配置采用可选的用户/metadata 根 `omd.toml`，按既定六层优先级运行，未增加配置框架。PLAN 差异仍为父侧既有记录。

父发现并窄修 B 本次遍历修改引入的具体回归：`grow -> grow/tail` 使“完整未解析路径”不断增长，无法命中 visited。只读有界 Git 包装器在第21次调用截停，目录未创建；直接路径对照成功。RED `/tmp/omd-parent-s4-fields-bn1x4id6/`，驱动 `/tmp/omd-s4-git-growing-cycle-probe.py`，修前文件 `/tmp/omd-s4-before-cycle-fix.QGuh6J/`。父仅改 `src/sources/git.rs` 和既有 `tests/git_source.rs`：先独立解析链接目标，只把当前目标解析栈作为循环依据，然后解析调用者后缀；保留根链接、合法重复遍历，不引入任意深度阈值。扩展原循环测试，未换测试框架。

最终候选 fmt/build/Git来源测试10项/full362（28 suites）/OpenSpec/diff-check 全 exit0，日志 `/tmp/omd-parent-s4-final-gates-174gtrgs/`。初次 focused 命令由父误写不存在的 project_remote，原错误日志保留；改用真实 remote_identity 后99项通过，不将命令错误归为产品回归。原六类 CLI、目录/根目录链接、增长循环和合法直接路径已复验；原共享 API 探针不改：路径保存 alternate.md，未知编码执行次数0，合法对照次数1，证据 `/tmp/omd-parent-s4-fields-g6etbf26/parent-assertions.json`。

编码六层、原字节、当前观察冻结、未知配置执行前拒绝及畸形配置零发布的父复验，以及 S2b/S2c/S3 原回归，记录 `/tmp/omd-parent-s4-final-remaining-krnbws9q/`。原 CLI/Git 探针及父新增配置脚本初次运行记录 `/tmp/omd-parent-s4-final-replay-2nmwq92c/`。不以计数替代这些行为断言。

**仅余待裁定的新增使用顺序：** 父另外尝试在首次 init 前，仅向新 `.omd/` 写编码配置 `omd.toml`，当前返回 usage/2 `metadata identity incomplete ... (state.toml required)`。此前只确认配置层级，未明确这种“配置目录存在但身份尚未建立”的初始化规则；不能擅自放松旧/损坏 store 的拒绝规则。原完整探针 `/tmp/omd-s4-parent-encoding-runtime-probe.py` 及失败 `/tmp/omd-parent-s4-fields-c6tohy9u/commands.json` 保留。另一个 `/tmp/omd-s4-parent-encoding-approved-checklist.py` 仅继续已批准的优先级检查，并明确将该额外场景记录为 pending_user_decision，未改原期望、未计为通过。下一步只请主人裁定是否支持配置先行；未授权新行为前不修改此入口。S5/S6、逐任务勾选和最终独立验收仍未完成。

### 配置先行新决定与 S5 派发

主人随后明确选择“配置可先写”：真正的新项目允许仅预放 `.omd/omd.toml` 编码配置后首次 init；必须没有旧身份、记录或已绑定映射，旧/损坏权威仍 fail-closed。该行为是独立补充，尚未实现；原 RED 和期望保持，不再标为待用户选择，也不再阻塞已批准 S5。OpenSpec 文字修订按 change-update 流程另行提出确认，不因行为选择擅改规划产物。

S4 冻结源码/dirty 状态/原程序：`/tmp/omd-after-s4.tdQ14x/`。S5 apply `ready`，具体 contextFiles 与状态保存 `/tmp/omd-s5-openspec-context/`；5/24 是继承勾选，不是新验收。

S5 是两个可单独门禁的串行契约：S5a 的输入、身份规范化、所有端点预检和保护发布必须共用同一有序计划，拆开会交叉拥有 main/cross；S5b 的查询表示和单位统计可在该交回后独立验证。两者共用 main，故不并行写；S6 最后集成，不重新解决组件设计。S5a 唯一 writer 合同 `/tmp/omd-s5a-cli-endpoints-task.md`；父侧持有规格、PLAN、任务勾选和验收权。沿用已批准 native Codex 单模型路线，不自动 provider fallback；不新建分支，不提交。

### S5a 固定清单父验收与一个窄修

workflow `e8c298e1-b509-47b2-819f-5bcc86339586` / child `1f57b658-aae8-4c19-a36c-2d803ff7939d` 已完成。父独立 fmt/build、focused159、full365（28 suites）、OpenSpec strict/no-interactive、diff-check 全 exit0；日志 `/home/xz/.cache/omd-parent-s5a-gates-ua8t95yk/`。HEAD 未变，暂存为空。writer 四套 focused 的实际和为138，不是交回报告误写的238。

旧 `/tmp/omd-after-s4.tdQ14x` 和临时合同经两种工具确认不可见，丢失原因未独立确定。一次有界恢复只取得保留转录、编辑/读取记录及现存测试证据，未恢复完整可逐字节核对的旧快照；不能声称已隔离全部 S5a 前后差异或豁免最终验收。当前候选、程序和 Git 状态已冻结到稳定位置 `/home/xz/.cache/omd-parent-s5a-frozen.SXnCCs/`，后续修复可据此精确比对。

父固定三组 CLI 检查 `/home/xz/.cache/omd-parent-s5a-checks.py`：混合四类方向/argv顺序和历史选择通过；不同 alias+同链历史/当前前缀重复 exit2，三方权威字节不变；相反方向及跨命令独立 link 通过。入向 link 的原始精确保护、peer reset 后选定 dangling 版本的 GC 保留也通过。真实晚期 I/O 通过子进程独立 RLIMIT_FSIZE 触发，BEGIN/正文/本地 link 已发布、peer 保护已持久后外部 link 写入返回 io_exec/5；成功对象逐一对应真实持久状态，开放块和 operation ID 正确报告。现有成功成员的 commit/link/protection 混合显示留给 S5b 结构化输出，不擅加 operation ID 必须持久在 BEGIN 的新要求。

**P1：入向 peer 关联的副本激活漏保护。** `src/main.rs` 的 Activate 只枚举 `link.target`，忽略新的 foreign source。独立 Bob 配置复制含入向关联的 S 后，T 激活成功并获新 store_id，但 protections=[]，实际 peer 既无 T 的 consumer 登记也无 T 的 inbound；S 保持不变。证据 `/home/xz/.cache/omd-parent-s5a-business-uw8v633p/{results.json,commands.json,incoming-authority-details.json,late-parent-assertions.json}`；旧重放 `/home/xz/.cache/omd-parent-s5a-business-hhss7r9q/` 保留。首次晚期 I/O 探针有变量遮蔽的父侧错误，v1 与修正重放日志均保存在冻结目录，未改产品断言。

本次只下发入向/出向统一激活保护及必要 peer 前置校验的窄修，合同 `/home/xz/.cache/omd-parent-s5a-frozen.SXnCCs/incoming-activation-fix-task.md`。不新增调查清单、不改源码以外的规格、README 或勾选；S5b、配置先行补充、S6 和最终独立验收仍待推进。

### S5a 窄修父验收关闭，转入 S5b

workflow `cf750162-678a-4cfe-87bd-ed18d21c6e51` / child `daf5b4f0-feba-4675-a96f-73069c95c9b8` 已交回。父对稳定树逐文件比较：修复只改 `src/main.rs` 的外部端点枚举与 `tests/cross_store.rs`；PLAN 为父侧派发前更新。源/目标均按各自版本收集必要 peer，保护仍使用旧创建 commit 与独立 link ID，未改 schema、GC 或来源规则。

父运行 fmt/build、入向激活两例、full367（28 suites）、OpenSpec strict/no-interactive、diff-check 全 exit0；独立确认 binary-only 适配器只改 BIN 且运行程序与现构建字节相同。固定8个父探针全部通过，包括不同版本/alias/prefix重复零写入、入向 dangling 保护、独立 Bob 的精确激活保护、被明确观察后的 GC 留存与真实晚期 IO/5。证据 `/home/xz/.cache/omd-parent-s5a-fix-review-uyomafgs/`；父重放 fixture `/home/xz/.cache/omd-parent-s5a-business-33eespgj/`。缺失/未观察/离线必要入向 peer 的永久测试也验证 T 及 Bob 权威配置零变更。HEAD 未变，暂存为空。

S5a 的这份固定组件清单与 P1 关闭，不重勾整个任务、不扩展新调查，也不抹去旧 S4 基线证据缺口。立即推进已批准 S5b。新稳定起点 `/home/xz/.cache/omd-after-s5a.hhq1pD/`；单 writer 合同 `/home/xz/.cache/omd-after-s5a.hhq1pD/s5b-query-coverage-task.md`，拥有结构化读侧表示及每来源/单位统计，不拥有 bootstrap 或 S6 文档/最终验收。

### S5b 父复核：固定 JSON／覆盖缺口，限界修复

workflow `6eec892d-2370-47d4-a7f0-2c89e0a4dd4a` / child `f77452db-4005-4481-b19d-36b8f04bc202` 已完成。父对稳定起点核对六个任务代码/测试文件；PLAN 为父侧记录。独立 fmt/build/full372（29 groups）/OpenSpec/diff-check 全 exit0，HEAD 未变、暂存为空。S5a 八例在新程序上通过，binary-only 适配差异已核对。证据 `/home/xz/.cache/omd-parent-s5b-review-s5b7f3zd/`；修前源码和程序 `/home/xz/.cache/omd-parent-s5b-frozen.oXYZTP/`。

组件判定 **changes required**，不勾任务。固定两组：
- **JSON／诊断**：JSON 参数解析错 exit2 但 stdout 为空；输出仍为旧 schema_version=1，未履行 design §7 升版；显式 unclean 后 verify/check exit1 却 diagnostics=[]，已知对象/commit 上下文缺失。规则 warn 的严重性也须与实际规则一致。
- **覆盖**：未标记来源忽略显式/配置编码，合法 windows-1252 被当 UTF-8，未知适用编码反而成功；同文件 text 解码失败掩盖仍可计算的 byte 分母；必要来源缺失时同单位 groups 仍把剩余来源报成100%，丢失未知分母。

固定驱动 `/home/xz/.cache/omd-parent-s5b-checks-v3.py` 与 `/home/xz/.cache/omd-parent-s5b-missing-aggregate.py`。原 v1/v2 留存：v1 的 byte 夹具长度小于复用 helper 固定范围，v2 只修该夹具错误；v3 仅将契约外的旧版本断言1改为批准的新2，未弱化业务负例。有效八例 RED fixture `/home/xz/.cache/omd-parent-s5a-business-1z2ct9wo/`；未知汇总 RED `/home/xz/.cache/omd-parent-s5a-business-yjwrsmk_/`。

真实晚期 I/O 父侧精确断言通过：成功 commit 加 link 创建记录恰等于新增已发布集合；成功保护恰等于新 inbound；失败 creator 不在 retained，失败 link 不在活动集合；开放块精确匹配并有 operation ID 与 io_exec/5。fixture `/home/xz/.cache/omd-parent-s5a-business-6csumd5y/`，断言记录 `parent-exact-partial.json`。父先前要求失败 creator 物理文件不存在的额外假设已撤下：未发布文件不等于发布成功；没有豁免实际权威状态检查，不为此新增清理工作。

调查到此停止，单 writer 合同 `/home/xz/.cache/omd-parent-s5b-frozen.oXYZTP/s5b-read-side-fix-task.md` 只修上述两组及直接回归。交回仅复核固定项关闭、修复影响范围内具体回归和既有必要约束；满足即结束组件复核，推进已批准配置先行补充与 S6。不是新 fresh-context reviewer 轮次，也不得另起无限组件循环。最终独立审查上限及旧基线证据缺口仍保留。

### S5b 固定修复验收关闭，推进 S6

workflow `882ead2f-1195-4443-8857-1a5319023ff7` / child `e117a066-35cb-445a-b756-a58b2061a906` 已交回。父对修前稳定树核对，实际修复只改 main/output/query_coverage 三文件；PLAN 为父侧更新。JSON 改为 v2，Clap 解析失败进入 usage/2 envelope；dirty 原因及对象/commit 上下文、warn 严重性接到真实诊断；覆盖复用既有编码选择，text/byte 失败独立，未知分母汇总标 partial 且 percentage=null。没有新增来源执行或改写历史。

父 fmt/build/focused8/full377（29 groups）/OpenSpec/diff-check 全 exit0。逐一核对四份适配 diff：仅 BIN 或其 helper 路径变化；固定 S5b 八例、缺失分母一例、S5a 八例全部通过。父再次独立对真实晚期 IO/5 的成功 commit/link/protection、失败 link 未发布、开放块和 operation ID 作精确状态断言。证据 `/home/xz/.cache/omd-parent-s5b-fix-review-hy0oehr4/`；重放 fixture 分别为 `/home/xz/.cache/omd-parent-s5a-business-0i00y2rg/`、`/home/xz/.cache/omd-parent-s5a-business-c6ube753/`、`/home/xz/.cache/omd-parent-s5a-business-23l9f5fs/`。HEAD 未变，暂存为空。

A1–A3/B1–B3 的固定组件清单关闭，不增加新调查或可选修复循环；整体 change 仍未接受，任务不因总数全绿自动勾选。配置先行行为早已批准，下一步仅确认其落入现有 design、发现规则与验收任务的文字修订，随后按已授权补充推进实现及 S6；不重新询问是否允许配置先行。旧基线证据限制和最终独立审查上限继续保留。

### 配置先行落文与恢复工作授权

用户确认 design、local-project-links/spec、任务3.1三处文字后表示不清楚同意内容且运行过久；父用“新项目先写编码配置再初始化，旧/损坏目录不能覆盖”解释同一行为并暂停。用户随后明确回复“ok 确认了 继续吧”。现已按确认内容仅修改上述三处，保留24个原任务及3.1未勾选；未借此扩大初始化权限或 Git 发布授权。

OpenSpec apply 为 ready，继承勾选5/24仍不等于本次验收。更新后的上下文及校验在 `/home/xz/.cache/omd-bootstrap-handoff-a9l0nmwd/`；OpenSpec strict/diff-check通过。父只补一个直接 RED：新默认 `.omd/` 仅含 windows-1252 编码配置，文本为原字节80/CRLF，首次init当前仍usage/2，配置目录字节未变。`bootstrap-red.json` 保留真实argv与输出。下一writer只实现该已批准入口并证明新配置正例与旧/损坏/绑定权威拒绝；不重开S1–S5，不扩大调查。完成后转文档/逐任务证据与最终独立验收。

### 配置先行组件关闭，进入收尾

workflow `f94ab23f-003d-477b-ab24-c13f3a3a4a1a` / child `bad5e245-d0be-4aaa-9ce6-67187d43ff75` 已交回。父冻结树比较仅 main/store/projects/project_context 四文件变化；配置目录资格与映射校验、创建前目录重验及既有编码验证接线，未修改持久格式。父已读实际变更与六个固定回归，独立跑 fmt/build/六例/full383（29 groups）/OpenSpec/diff-check 全通过。另重建原正例，验证 windows-1252、80/CRLF 原字节、完整 hash/length、配置字节不变与 verify 成功。证据 `/home/xz/.cache/omd-parent-bootstrap-review-76limdcp/`；HEAD 未变，暂存为空。

固定组件通过不等于整个3.1或change接受。后续单writer只做双语README、受影响说明、24项证据对应与必要的文档/集成验证；产品Rust代码冻结，不再追加组件调查。缺失历史基线及无法追溯的原探针出处继续明确列为证据限制，不能假称恢复。父核对收尾交接后进入既定最多三轮的最终独立验收；无提交、推送或归档授权。

### S6 文档交回与最终验收入口

workflow `fc110b53-470d-481f-831d-050d600e3252` / child `93ed48c8-5155-4bf1-9b10-6d266a6cb6b4` 已完成。父确认只改10个文档，产品源码/测试/依赖和OpenSpec任务未变，现程序与S6冻结程序字节相同。父从两份最终README直接提取完整示例重跑：均exit0，真实IDs、四份caller evidence（publication 1/2/3/7）、改内容后verify/check exit1均验证；32个本地链接有效，fmt/OpenSpec/diff-check通过。复用未改产品代码的父full383门禁，不重复制造测试证据。父证据 `/home/xz/.cache/omd-parent-s6-review-czmpc2_g/`。

writer提出23/24 PROVEN、5.1因原RED证据丢失为PARTIAL；这是待独立核验的声明，不是父结论，未据此修改任务勾选。原abcd→abXYcd脚本/行出处也未恢复；当前负例与新观察正例应分别核对。进入本次修复周期第1轮fresh-context最终审查（最多3轮），明确区分实现缺陷、必需证据缺口、历史证据限制和可选项。必要证明仍不足则最终BLOCK，不以新增无终点阶段替代结论。

### 最终裁决：当前实现通过检查，完整验收 BLOCK；本轮停止

第1轮 workflow `73b57498-2a3a-4e8a-8108-c0258af6bfbf` 的 Codex reviewer 实际检查候选后遇到 429 usage_limit_reached，未产生最终报告；因此消耗一轮，父先前“未启动、不计轮次”的说明错误，已更正。主人明确授权改用 Kimi K3，第2轮 workflow `8fb62d4e-c3da-413c-bdfc-72259d660b9f` / child `dc38b49c-b899-44fa-ae9b-f77154a90970` 完成只读审查。报告在对应 managed outputs 下 `authorized-repair/final-review-round2-kimi.md`。

reviewer 建议 implementation OK / evidence及merge OK with notes，未发现当前必修缺陷。父确认审查后候选所有受管文件与冻结树相同，再核对原始日志及README receipts：383项Rust测试/29 result groups，另18个BDD scenarios和79 steps；报告480是三者混加，不是独立测试数。§5.4引用ID确实存在于S6的readme-en-final/readme-zh-final/evidence-summary.json；reviewer混淆了父侧不同回放，所谓ID证据丢失不成立。

父据逐任务证据与独立结论勾选23/24，5.1保留未完成。当前实现检查无必要修复；历史原RED及旧abcd→abXYcd探针出处未恢复，不伪造、不重命名为新正例。完整change验收为BLOCK（5.1历史证据条款未满足），不采纳reviewer将该缺口降为整体通过附注的建议。两轮即停止，不花第3轮重复历史考古。此后仅更新任务/证据/交接状态，源码及测试不变；截至本次验收裁决，没有commit/push/merge/archive。
