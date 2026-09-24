# Tasks

## 1. audit/note 链基础设施

- [x] 1.1 扩展 CommitKind 与对象 key：`audit:<root>`/`note:<root>` 前缀、`AuditInit`/`AuditPatch`/`NoteInit`/`NotePatch`；state tips/locations 挂载；链式 append 复用 previous_id/reset 语义。验证：单元测试——init 造根、patch 追加 previous 链、reset 撤走 audit commit 后 tips 移动。
- [x] 1.2 note 平面格式拒绝：读 note 集合的命令遇 `notes/<id>.toml` 报错指路径，不解释不迁移；新 note 只写链式记录。验证：集成测试——旧文件存在时 note list 报错、原字节不变；新 note add/patch 走链。

## 2. audit 生命周期与涂色检查

- [x] 2.1 `omd audit add <seed> [--direction both|upstream|downstream] [--text]`：创建 audit 链 init commit（种子、方向、正文），默认 pending。验证：集成测试——add 产出可 log 的链根；list --status pending 可查。
- [x] 2.2 `omd audit show <audit-id>`：按记录的方向遍历 link 图（both 时正反两个独立子图），逐边 judge_link（L0–L2）、逐点 endpoint_alive，结果作为证据输出；不写回链。验证：集成测试——构造上游 obliged/下游健康的图，both 分开报告；broken 边 exit 1。
- [x] 2.3 `omd audit pass|fail|pending <audit-id> [--text]`：追加结论 patch commit。验证：集成测试——patch 链接前一条；list --status 三态过滤各自命中；fail 结论命令 exit 0。
- [x] 2.4 `omd audit list`：`--status`/`--start`/`--end`/`--touched-start`/`--touched-end` + 分页（默认 20 上限 100，提示性游标）。验证：集成测试——时间窗命中 audit 自身与涂色端点两种过滤；时间戳只读不写。

## 3. 双引用与 wiki

- [x] 3.1 Link 端点扩展到 audit/note commit：`audit:`/`note:` key 合法端点，端点活性/reset 撤走判 withdrawn。验证：集成测试——修复 commit link 到 audit commit a2；a2 被 reset 后该端点 withdrawn。
- [x] 3.2 双引用交叉一致性检查：同一处关系同时有 Link 对象与载荷 `audit:<id>` 引用时，两边指向必须一致；不一致报检查失败项。验证：集成测试——a2/a3 不一致场景报错；一致场景通过。
- [x] 3.3 audit wiki 正文引用：`audit:<commit-id>` 记号解析——验证 id 在某 audit 链上存在、输出指向的链与 commit；不存在报点画错。验证：集成测试——正文引用 a2，目标追加 a3 后仍解析 a2；引用不存在 id 报错。

## 4. 工作流闭环与回归

- [x] 4.1 unclean 索引闭环：`commit unclean --reason "audit:<id>"` 造脏；理由可解析回 audit commit；修复 commit 通过 Link + 载荷双引用回链。验证：集成测试——从 audit 发现问题到 verify 报脏到修复回链全程走通。
- [x] 4.2 `omd note` 命令面切换到链式实现：add/patch/delete/list 语义保持（修订序按链），输出投影适配。验证：集成测试——时钟乱序场景修订序仍按链；note list 输出链式投影。
- [x] 4.3 文档同步：skills/omd SKILL.md 与 capabilities.md 增补 audit 工作流（涂色、三态、双引用、unclean 闭环）；README 双语运行边界段更新。验证：本地链接解析、CLI help 与文档一致。
- [x] 4.4 全量验证：`cargo fmt --check`、`cargo build --locked --bin omd`、`cargo test --locked`、`openspec validate audit-ledger --strict --no-interactive`、`git diff --check`、教程回放。验证：全部通过。
