# Tasks

## 1. Link 健康判定模块

- [x] 1.1 实现 link 健康五态判定（healthy/obliged/stale/withdrawn/broken）+ unchecked 计数：L0 结构（记录可读、端点 key 合法）→ L1 活性（selected_version 有效、非 dangling）→ L2 义务（link_pending 非空 → obliged 优先于 stale）；command 端点未获准运行计入 unchecked。每链早退，停于第一个失败层并记录该层。验证：单元测试覆盖五态各一例 + unchecked + 早退（broken 链不触发 L1/L2 读取）。
- [x] 1.2 实现 SCC 凝缩拓扑分层：Tarjan 或等价 O(V+E) 算法，source→target 有向图，根 = source 侧且非任何 link 的 target；环成员同层；peer 端点作不透明节点。分层为派生只读视图，不持久化。验证：单元测试——无环图分层正确、三节点环同层、环下游层号严格更大、自指环不失败。

## 2. `omd links` 查询动词

- [x] 2.1 实现 `Cmd::Links`：默认概要输出（total、by_status 含 unchecked、by_stratum，无明细）；`--status` / `--node`（双向匹配，peer 端点按持久 id 匹配） / `--id`（单链全细节） / `--stratum <n>` 过滤下钻。验证：集成测试——大 store 概要输出与规模无关（断言无 items 键）、各过滤参数命中正确集合。
- [x] 2.2 实现分页契约：`--limit` 默认 20、上限 100（超限截断并如实报告），响应含 total/has_more/next；游标 token 编码 (filters, offset)，重放按当前 state 重算，漂移时输出 note 不拒绝。`--brief`（默认：link_id/status/source/target/stratum 五字段）/ `--full`（完整投影 + status + stratum）。验证：集成测试——57 条命中默认 20 + has_more + 游标翻页、limit 500 截到 100、游标漂移场景（两页之间写入新 link）报 note 不拒读。
- [x] 2.3 实现单链 `--id` 全细节：端点、钉住版本、创建 commit、状态、所在层、pending 明细。验证：集成测试——`--id <lid>` 返回完整投影且不含其他 link。

- [x] 2.4 重构 links CLI 为显式动作：`omd links`（概要，拒绝明细修饰参数）、`omd links list`（分页明细，无过滤=枚举全部）、`omd links show <id>`（单链全细节，拒绝过滤组合）。适配 tests/link_query.rs 到新动词面。验证：`cargo test --locked --test link_query` 全过；概要模式带 `--limit`/`--full` 报 usage 错误；`list` 无过滤可分页枚举全部。
## 3. 洪泛修复：verify / check / list

- [x] 3.1 `verify --json` / `check --json`：`data.links`/`data.objects` 改为 `link_count`/`object_count` 计数；新增 `--links <filter>`（node:<id> / status:<state>）与 `--objects <filter>`（path:<glob>）显式请求分页明细（复用 `omd links` 的过滤函数）。dirty/cosmetic/规则/退出码不变。验证：集成测试——大 store verify 输出含计数无全量数组；带 `--links node:<id>` 返回该节点明细；脏 store 退出码不受过滤参数影响。
- [x] 3.2 `omd list` 改概要 + 游标分页：对象/link/open block 计数概要，`--dangling` 分支同样分页。验证：集成测试——list 输出计数无全量 tips/objects/links 数组；游标遍历拼全量一致；现有依赖 list 输出的测试适配后全过。

## 4. 回归与文档同步

- [x] 4.1 适配现有测试：grep 全测试套对 `data.links`/`data.objects`/list 全量数组的断言，改为计数或分页断言。验证：`cargo test --locked` 全过。
- [x] 4.2 同步 skill 与 README：`skills/omd/SKILL.md` 与 `references/capabilities.md` 增补 `omd links` 概要→下钻工作流与分层检查建议（上游优先、环合法同层）；README 双语运行边界段落更新 JSON 输出契约变化。验证：本地链接解析、提及新动词的段落与实际 CLI help 一致。
- [x] 4.3 全量验证：`cargo fmt --check`、`cargo build --locked --bin omd`、`cargo test --locked`、`openspec validate link-query-and-stratified-check --strict --no-interactive`、`git diff --check`、教程回放 `python3 examples/regression/check-tutorials.py`。验证：全部通过。
