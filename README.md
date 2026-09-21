# oh-my-document（omd）

面向人类与 AI 的、可配置的文档—代码关联跟踪工作流。Rust 核心 + CLI。

> **状态：核心已实现，可用于试用。** 248 个单元/集成测试 + 18 个 BDD 场景绿；`cargo fmt` / `cargo clippy -D warnings` 干净。137 个规格场景全部映射到已执行测试或显式平台记录（见 [spec-traceability.md](spec-traceability.md)）。

## 安装与试用

```bash
cargo install --path .   # 或 cargo build --release 后使用 target/release/omd
```

无 Git 仓库要求；每个项目在自己的 `./.omd/` 元数据目录下工作（也可 `--meta <dir>` 或 `OMD_META` 指定）。

### 30 秒上手

```bash
cd your-project

# 1. 登记来源（文件或命令输出）
omd init docs/spec.md
omd init src/main.rs

# 2. 提交要跟踪的内容范围并建立关联
omd commit commit docs/spec.md --range 8-25 --reason "algorithm A described"
omd commit commit src/main.rs --range 0-3 \
    --link-from "docs/spec.md@text:8-25" \
    --reason "implementation of algorithm A"

# 3. 声明规则：spec 变更必须被 code 覆盖
omd commit tag docs/spec.md --tag spec
omd commit tag src/main.rs --tag code
omd commit scope_adjust src/main.rs --rule "spec->code" --level warn

# 4. 检查关联覆盖率 / 校验内容是否漂移
omd check     # 规则覆盖率报告（如 spec->code: 80%）
omd verify    # 内容变化检测：范围被编辑 → dirty，需人工确认
```

文档改了一个词之后：

```bash
$ omd verify docs/spec.md
ok: false | dirty: ["range:docs/spec.md@text:8-25"]   # 变化被确定性定位
```

处理变化要么修改代码并重新提交，要么 `omd commit commit <path> --id <旧提交> --range <新范围> --reason "<为什么>"` 显式更新关联。

## 核心概念

- **跟踪单位是 range（内容范围），不是文件。** 文本按解码后 Unicode 字符序号（`file.md@text:8-25`），字节模式按原始偏移（`file.bin@byte:0-128`）。
- **link 连接 range ↔ range**，按 `link_id` 识别；同一对端点可以有多条独立 link（各带理由）。上游变化在下游产生待处理义务（`adapt` 按 link_id + 理由 + 选定变更逐条处理）。
- **三态标记：** 空 / 脏 / 已确认。只有未过期的已确认算覆盖；跳过（`--skip`）永远不等于已确认。
- **变化检测 = 内容 hash + Myers diff**，不依赖 Git。commit id 由 `SHA256(salt ‖ frame(prev) ‖ frame(timestamp) ‖ frame(content) ‖ frame(JCS payload))` 派生，TOML 字段顺序不影响 id。
- **ATOMIC 块：** `commit begin` / `commit end` 之间的一组提交作为一个单元；reset 只能落在 BEGIN/END 边界（占位标记回退一步到直接前驱），不能 reset 块内成员。
- **check ≠ verify：** check 报告关联覆盖率（规则驱动），verify 报告内容是否与记录一致。两者独立失败，互不代替。

## 主要命令

| 命令 | 用途 |
|---|---|
| `omd init <path>` | 登记来源（不可叠加：已跟踪路径再 init 报错；`--source-ref 'command::<exe>::<JSON argv>'` 跟踪命令 stdout；`--encoding` 指定文本编码） |
| `omd commit commit <path> --range S-E --reason R` | 提交范围（`--id` 追加到已有链 = 修改范围；无 `--id` = 同坐标新独立对象） |
| `omd commit begin/end <path>` | ATOMIC 块边界 |
| `omd commit reset <path> --reset-target <commit-id>` | 重置到指定提交（边界标记回退一步；被移除段内的 link 一并撤回） |
| `omd commit adapt <path> --link-id L --changes c1 --reason R` | 处理 link 上的待处理变更（`--stop` 清全部） |
| `omd commit tag/rule/skip` | 规则工作流：标签、`A->B` 覆盖规则（`--level warn/fail`）、显式跳过 |
| `omd verify [path]` / `omd check` | 内容校验 / 覆盖率检查（`--run-command=true` 允许 verify 重跑命令源并比对 stdout） |
| `omd log/tree/list` | 链历史、层级树（`--level file`）、状态查询（`--dangling`） |
| `omd note add/patch/list <commit-id>` | 提交上的 append-only 注记（修订按发布序，不按时钟） |
| `omd replace <commit-id> --source <ref>` | 仅在字节完全一致时把获取来源重绑到新路径（id/links/notes 不变） |
| `omd register` / `commit ... --xlink-to peer:<store>:<file>@<range>` | 跨仓库关联（各自元数据独立，绝不合并；inbound 凭据先于对方发布持久化） |
| `omd gc` / `omd reindex` | 收集悬空记录（保护闭包内保留）/ 重建派生索引 |
| `omd import/remove/delete/rename/copy` | 统计范围与生命周期（均为 commit 的别名族） |

所有命令支持 `--json`（stdout JSON 信封，stderr 独立诊断），退出码：0 成功 / 1 一般错误 / 2 用法 / 3 版本冲突 / 4 锁冲突 / 5 执行失败。

## 并发与安全

单写者锁（`write.lock`）；并发更新直接拒绝，不合并、不覆盖。`state.toml` 经 tmp→fsync→rename→dirsync 原子发布。读取方在打开时校验 tip 完整性——被篡改的元数据报错而非静默解析。命令源默认不执行：只有显式 `--run-command`（或配置）才重跑命令，一次调用的许可不带入下一次。

## git:: 来源（可选）

`--source-ref 'git::<40-hex-commit>:<path>'` 通过只读 Git plumbing（`cat-file`）读取历史 blob：精确 40-hex、原始内容、有界符号链接解析，永不回退到工作区当前内容。工作区文件跟踪与 Git 仓库状态完全无关。

## 未实现 / 明确延后

- 远程身份子系统（URL↔声明映射、SSH/HTTPS 等价性）— 规格可选项，未设计
- 命令捕获期间的存储故障注入测试（无进程内故障接缝）
- 跨平台持久化验证（当前仅 Linux 单机验证；`fsync`/`rename` 语义在其他平台未测）
- 可选 programming-thinking（Lean）产品 skill — 契约已确认，另行授权后实现

## 文档导航

| 文档 | 用途 |
| --- | --- |
| [AGENTS.md](AGENTS.md) | AI 工具的阅读顺序与协作约定 |
| [spec-traceability.md](spec-traceability.md) | 137 规格场景 ↔ 已执行测试的逐行台账 |
| [需求基线](docs/requirements.md) | 用户确认的决定；R 编号用于追溯 |
| [来源与坐标](docs/source-model.md) | 引用形式、编码、command 契约 |
| [存储与路径](docs/storage.md) | 路径规则与布局 |
| [候选架构](docs/architecture.md) | 模块职责 |
| [待决事项](docs/open-questions.md) | Q 编号；不得由实现悄悄拍板 |
| [验收场景](docs/acceptance.md) | 测试场景 |
| [handoff](docs/handoff.md) | 交接说明 |

## 许可证

尚未选择。仓库公开不等于已授予任何开源许可。
