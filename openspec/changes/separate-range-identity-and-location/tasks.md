## 1. 结构化对象与版本解析

- [x] 1.1 按 design 第 1、7 节定义具名对象引用、commit 引用与版本位置字段，复用现有 Range，提升持久 schema；验证 TOML/JSON 往返、未知字段拒绝、旧格式拒绝且原目录字节不变，不加入迁移或兼容读写。
- [x] 1.2 实现统一 store/commit→链首解析，区分对象类型、当前 tip 与有效范围版本；以完整 ID、唯一/歧义前缀、缺失前驱、错误 store、文件 ID 冒充范围及 dangling 历史测试验证，不回退路径或最新 tip。
- [x] 1.3 将 state 的 tips、mounts、dirty、open_blocks 与文件范围恢复表切换到对象引用；验证同坐标两条独立链、缓存删除重建与重开存储后身份/挂载/版本不变，索引篡改不能改变原始记录含义。
- [x] 1.4 保持 canonical hash 分帧与全部不可变输入规则；用黄金输入向量、首提交无自引用、同内容独立新操作、映射移动/复制/note/同内容 replace 的复算测试，验证 project_id、本机路径及自身 store 归属未进入业务 hash。

## 2. 范围生命周期与关联

- [x] 2.1 将文件 init、范围新建/续改、rename 和 tombstone 接到结构化模型；验证重复 init 拒绝、同坐标独立、重叠范围、坐标续改及腾空路径复用不混合历史或 link，旧 tip 续改拒绝且零发布。
- [x] 2.2 按操作类型折叠有效范围版本与业务责任，区分 BEGIN/END 空正文；验证首 BEGIN 为对象身份、正文前零覆盖、END 后仍能取得正文依据，以及 clean/unclean/link 不因正文未变被忽略。
- [x] 2.3 更新 range/file reset 的对象解析和恢复映射，保留 --reset-target；运行首 BEGIN 退空、嵌套/相邻边界只退一步、内部普通点拒绝、文件恢复 END 不再退一步及子目标非法时整次零变更的测试。
- [x] 2.4 将 link 创建及适配改为对象端点与独立版本依据；验证链推进不新建 link、跨命令同端点 L1/L2 并存、只适配指定 link/变化/理由、反向查询不产生反向关系、必要 dangling 引用不能被换成新 tip 消除。
- [x] 2.5 更新 GC 及断链诊断对链首/前驱/范围版本的依赖；验证仍保留链的 root、正文版本、块结构与完整来源受到保护，未被引用的 dangling 仅由显式 gc 清理，查询/reset 不触发清理。

## 3. 来源、本机映射与跨存储

- [x] 3.1 用封闭的 file/command/git 字段模型替代复合来源字符串，分开观察定义与恢复 binding；验证 file 默认值、类型不兼容/缺字段在采集前拒绝、text/byte 单位与编码优先级、原字节/BOM/换行保留，既有 command 不因省略类型变成 file。补充验证配置先行首次 init：仅含 `.omd/omd.toml` 且无既有身份、记录和绑定映射的新项目可使用预置编码初始化，配置原字节保留；已有、损坏或已绑定目标拒绝覆盖初始化且权威目录/本机登记不变；读取配置不触发来源执行，既定初始化采集许可与次数不变。
- [x] 3.2 将独立 executable/JSON argv 接到既有执行器，不新增执行时机；通过计次来源程序验证显式 init/replace 仅执行一次、所属项目 cwd、字面 argv、verify/check 许可优先级、失败保留、合法空输出，以及 log/tree/缓存重建完全不执行。
- [x] 3.3 将 Git 历史及 replace 接到项目 alias、确切 Git ID 和版本内路径；验证历史与当前路径不同、未提交修改仍被观察、HEAD 移动不替代文件、缺 Git 对象失败且不联网、同片段但完整内容不同的 replace 拒绝、共享 V 与等 hash 的 W 不混改。
- [x] 3.4 分开共享 alias 逻辑绑定与本机项目根/元数据目录映射，隔离实例缓存和观察凭据；用两个不同根、同项目的两个 worktree、缺失/歧义/错误映射及映射更新后的旧凭据验证位置可移植、不串缓存、不静默换绑、不重算历史 ID。
- [x] 3.5 完成可选 remote 名/URL 的独立登记、显式认可映射及诊断；验证未配置时普通目录无 Git 依赖，配置后匹配通过、缺失/不匹配拒绝、SSH/HTTPS 不自动等价、无 force、无 clone/fetch。此次要求不能沿用前序 deferred 标签宣称完成。
- [x] 3.6 将 peer/inbound 的定位接到本机映射，保留先保护后发布及副本激活规则；验证 design 第 6 节 Alice/S、Bob/T 的复制→登记→续改路径：旧 ID 保留、T 可独立追加、S 不变、原跨 store 引用不改指 T，必要 peer 离线阻止不完整激活且 GC 保守保护，无关 peer 离线不阻塞本地操作。

## 4. CLI、结构化输出与其他消费者

- [x] 4.1 在现有 CLI 中实现独立路径、--project、双端点 --range、--mode 及来源字段；保留 --id 的 tip 语义和 --expected 前置条件。用真实 CLI 测试缺字段、越界、未知类型、旧 source-ref/source-json/复合端点拒绝，确认错误前不运行来源命令或写入 BEGIN。
- [x] 4.2 实现可重复 --link-from/--link-to 及每项固定 alias/commit 的跨 store 形式，并更新显式 link 入口；验证混合方向与 store、不串参数、同方向同对象的不同 alias/前缀/版本一律在任何发布或 inbound 写入前拒绝，相反方向和跨命令重复仍允许。
- [x] 4.3 更新 JSON envelope 中 node、范围、link、覆盖及诊断字段，保持 ID/大整数字符串和不适用 null；通过 JSON 断言验证 root/tip/有效范围/source version 分开、解析错退出 2 而非 lock_conflict、真实锁冲突退出 4、组合中途失败保留实际成功 ID 与开放块。
- [x] 4.4 将 tree/log、import/remove/tag 与 check 消费者接到对象引用和结构化位置；验证重名/重叠对象不混合、挂载不沿 link 展开、未标记内容仍进分母、重叠覆盖取并集、byte 不套文本空白过滤、覆盖不清除独立责任，移除旧 key 反向解析路径。

## 5. 端到端验收与文档

- [ ] 5.1 在现有 Rust/CLI/BDD 设施补齐特殊字符路径的黑盒回归，先保留可复现失败再验证修复；覆盖 `we@ird #1%.md`、Unicode、当前平台允许的冒号及 -- 边界，通过 init→范围→关联→rename→查询验证身份不随显示位置变化，不新增测试框架。
- [x] 5.2 独立复现普通 spec→code 关联示例中的 `version record missing`，针对真实原因补足版本解析并验证：依据齐全且其他检查满足时 verify 成功，真正缺版本时仍失败；不要把重构、场景计数或错误字符串消失当作正确性证明，也不假定它与路径解析同根因。
- [x] 5.3 跑跨模块回归：同坐标独立对象、范围续改、同端点多 link、嵌套 reset、同内容 replace、复制续改与跨 store 引用、删缓存恢复，以及 stale/lock/单次发布失败；记录实际命令、退出码和断言，完成 `cargo fmt --check`、`cargo build --locked --bin omd`、`cargo test --locked`，如实区分新回归与既有未验证平台/故障注入项，不把 skipped/deferred 记作通过。
- [x] 5.4 更新英文 README.md 与独立 README.zh-CN.md 的输入示例、互链、已知限制及本机映射/可写副本边界，并同步受影响的 source-model/storage/handoff/AGENTS 说明和 spec-traceability.md；验证两种 README 的完整示例使用真实返回 ID 均可运行、Markdown 本地链接有效，只有实际修复后才撤下缺陷告示，历史决定保留其被取代说明。
- [x] 5.5 汇总本 change 五份增量规格与仍适用基线的验收对应，逐项核对实现证据和剩余限制；运行 `openspec validate separate-range-identity-and-location --strict --no-interactive` 与 `git diff --check`，确认无旧来源语言的活跃说明、无隐式迁移/联网/换绑；未完成或延期的行为不得勾选对应任务。
