## 1. Spike：difftastic 可用性探测

- [x] 1.1 在隔离目录实测 `difft` 的机器可读输出形态（`--display`/JSON 选项）、exit code 语义、纯空白变化判定、语法回退到文本 diff 时的表现、版本输出；记录可解析的判定路径，不确定的输出形态一律归 Unclassified 的结论依据。交付：spike 记录写入本 change 目录 `spike-difftastic.md`，含实测命令与输出摘录。

## 2. 库层分类抽象

- [x] 2.1 在 `src/relations/` 新增 `DiffClassifier` trait（输入：旧完整内容、新完整内容、文件名提示；输出：`CosmeticOnly | Changed | Unclassified(reason)`）及纯 Rust 单元测试（含 Unclassified 保守路径）；验证 `cargo test --offline --locked` 相关组通过。
- [x] 2.2 实现 CLI 层 difftastic 适配器：固定 argv、带真实扩展名的临时文件、版本采集、`--version` 调用一次并缓存于本次进程；验证适配器单元测试覆盖启动失败与非零退出归 Unclassified。

## 3. verify/check 过滤语义

- [x] 3.1 `verify --difftastic`：在写锁观察流程内按文件级调用分类器，报告拆分为 `data.dirty`（结构变化 + 无法分类）与 `data.cosmetic`（携带工具身份/版本与旧/新版本依据）；验证纯格式风暴 fixture 下 dirty 空、cosmetic 非空、exit 0，无参数调用行为与现状逐字节一致。
- [x] 3.2 `check --difftastic` 共用同一分类与分桶；验证 check 报告含 cosmetic 桶且失败退出码仅由 dirty 决定；byte 模式与无法解析扩展名 fixture 保守留在 dirty 并带原因。
- [x] 3.3 工具缺失/失败 fixture：`--difftastic` 时 difft 不在 PATH，验证诊断如实报告、相关范围归无法分类保留在 dirty、命令不崩溃且 JSON envelope 完整。

## 4. `commit cosmetic` 收尾命令

- [x] 4.1 实现 `commit cosmetic <path>`：新凭据 + 写锁内对当前内容重新分类；与过滤视图不一致的范围拒绝续改并如实报告；验证"过滤后内容又被改动"fixture 拒绝发布。
- [x] 4.2 通过重分类的范围在一个 ATOMIC 块内逐条续改到新坐标，分类证据（工具、版本、旧/新版本 ID、判定）写入 commit payload；验证格式风暴 fixture 一次命令收尾后无过滤 verify exit 0，下游待办保留。
- [x] 4.3 晚期 I/O 失败沿用部分发布契约：续改失败的范围进 `refused` 如实报告、其余成员照常进 ATOMIC 块并闭合、operation_id 输出。集成测试 `stale_cosmetic_view_is_refused` 验证凭据过期拒绝（version_conflict/重分类拒绝均诚实呈现）。（注：fault-injecting probe 的真实 fs 注入未单独实测，cosmetic 路径复用 `commit_source`/`commit_marker` 的既有部分发布行为。）

## 5. 集成与回归

- [x] 5.1 端到端 CLI 回归：格式风暴 → `--difftastic` 只剩结构变化的 dirty → `commit cosmetic` 收尾 → 无过滤 verify exit 0；JSON 断言与退出码已在临时项目 fixture 逐步验证。
- [x] 5.2 权限回归：单次 `--difftastic` 不携带到下次调用（无 flag 时 dirty 报全量、无 cosmetic 键）；读取/查询/log/tree/reindex 不触发外部工具（空 PATH 下 tree 正常）；验证无 difftastic 环境 + 无参数时全部行为与现状一致。
- [x] 5.3 全量验证：`cargo fmt --check`、`cargo build --locked --bin omd`、`cargo test --locked`（393 过）、`openspec validate difftastic-cosmetic-filter --strict --no-interactive`、`git diff --check` 全部通过，教程回放 `python3 examples/regression/check-tutorials.py` 13 块×2 语言不受影响。

## 6. Skill 与文档同步

- [x] 6.1 改写 `skills/omd/SKILL.md` 复查章节为优先级队列（先 `--difftastic` 过滤 → 逐条审 dirty → 收尾批扫 cosmetic → 完成定义仍是无过滤 verify exit 0），写死三条纪律：先过滤、收尾不等于丢弃、工具不可用诚实降级全人工；更新 `capabilities.md` 功能分支表与契约链接。
- [x] 6.2 同步 `docs/handoff.md` 能力边界与 README 双语"运行边界"段落提及 `--difftastic` 的一次性许可语义；验证全部本地链接可解析。
