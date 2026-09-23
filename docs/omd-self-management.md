# OMD 自管理

本仓库用 OMD 连接真实规范、实现和测试。操作入口：[OMD skill](../skills/omd/SKILL.md)。目标是理解系统及变化影响，不只是更新文档；旧 change 的 5.1 历史证据缺口不因此消失。

## 位置与统计范围

- 权威：项目 `.omd/`；默认用户配置保存本机登记，默认缓存可重建。不要手改权威记录。
- 使用当前源码离线构建的 `target/debug/omd`；PATH 中旧版未更新。旧版会拒绝同路径统计/正文双对象，不能混用或删记录绕过。
- 已实际核对统计清单：`openspec/`、`docs/`、`src/`、`tests/`、`features/`、`skills/omd/`、`README.md`、`README.zh-CN.md`。
- README 直接按文件 import；没有 import 根目录，不需要 exclude 绕行。`.git/`、`target/`、`.omd/` 未纳管，这是本次明确范围，不是工具隐藏排除。
- import 不等于 init 或确认正文。除下列关联文件和两份 README 外，其余文件没有批量 init；未复核内容保留缺口。

## 首条关联：多字节文本坐标

建立时采用 UTF-8 Unicode scalar `[start,end)`；当前位置与 tip 以查询为准。

| 角色 | 文件与建立时范围 | 链根 ID |
| --- | --- | --- |
| 规范 | `openspec/changes/separate-range-identity-and-location/specs/managed-content-tracking/spec.md` `[3083,3291)`：Multibyte text 场景 | `23dafa83b0a8d897d243b93cd4718ac593d928f8f238199ad79a54ab3ee6921f` |
| 实现 | `src/relations/range.rs` `[2323,3490)`：`text_len`、`text_slice` | `b6943ae51da5dc41d4ab68f879c7e6c40ff02cb4b3239c5a1e5ec7523ce6cff7` |
| 测试 | `tests/ranges.rs` `[296,597)`：`text_ranges_count_scalar_positions_not_bytes` | `34c396b1a678950659a548dcf0fa948b102d72ec946b8d3cabaadb435d37ea5b` |

store：`807eb8461236449b83038cb47c18e3af`。
规范→实现 link：`016d01f925b45a12b9a8638b987dfab4`；实现→测试 link：`7670cd146d71f1a4084baec594fb2ee8`。

实际查询已验证这些对象与 link；只复核该多字节文本场景，不声称全部编码性质通过。带 link 的组合有 BEGIN/正文/link/END，链根不是始终可写的 tip。

## 实际使用发现与修复

直接 import 文件原先可能报缺少 `file:pending` 观察，或成功却不进入清单。经所有者确认，支持文件与目录，并让统计对象与正文对象同路径独立存在。保持旧记录及 ID，不迁移、重建或删除真实历史。

同时修复目录筛选更新未生效：查询原先固定读取链首筛选，现在读取有效链上最近的 import/remove，文件清单与 tag 覆盖共用解析。根目录标签成员保持相对路径。断链/缺失统计返回 incomplete 和失败，不静默成功。

证据：

- `cargo test --offline --locked`：386 项 Rust 测试通过，另有 18 个 BDD 场景、79 个步骤通过。
- 新 CLI 回归先 RED 后 GREEN：`file_import_is_independent_of_content_tracking`、`missing_imported_file_fails_without_silent_scope_loss`、`revised_root_import_updates_listing_and_tag_coverage`。
- 隔离旧程序生成 init→import 历史，新程序追加独立统计对象，旧记录字节不变；旧程序打开新状态明确拒绝，而非误选对象。
- 真实两份 README 已在 `data.check.files` 中核实；原链根及两条 link 保持不变。
- `cargo build --offline --locked --bin omd` 成功；操作 JSON、RED/GREEN 输出及兼容探针摘要存于本机 `~/.cache/omd-self-use-20260923/`。本机日志不是共享权威或唯一内容基线。

## 本次修复的关联与实际复核

以下是修复后补建的关联，不冒称修改前已有跟踪：

- import 契约最初 `[26,1144)`，保留旧目录场景后显式复核为 `[26,1566)`：链根 `9e34593586578322acbfbf2a45aa96778ac27acff9f01e1d2535ac3acc953ad2`。
- `src/records/store.rs` 定位方法 `[39690,40529)`：链根 `62fc51578e58240dd037ccee1be22db30de1e7a0cd5f2574beaf6380bdce8bba`。仅表示定位职责，不声称包含全部 CLI/pipeline/scope 实现。
- `tests/query_coverage.rs` 生命周期与缺失回归 `[4382,7031)`：链根 `a1eeeddd1494fa431babe7c4f61ec8cc63d1ebb03c52bf631b4ab116157041c0`。

三者按规范→实现→测试关联，加上最初一组，目前共六个范围、四条 link。

新增契约使原 Unicode 场景移到 `[4201,4409)`。verify 的 locate 曾报告 moved/needs review，dirty 为空不能忽略。逐字比较持久旧正文与当前片段后，确认完全相同，再以原 tip 显式续改坐标；新 commit 为 `636b19e5709da34f22247ad0d1b61246e57999c3000c77f977af6989af500082`，链根不变。严格规格校验随后要求保留原目录场景，已恢复并再次复核：Unicode 当前范围为 `[4623,4831)`，tip 为 `2ee5b0ba0769a8c939bbdb8c5559726eddaab67abf9a060c88cab8d0ffdb05e4`；import 契约 tip 为 `7c63ae23bc56aeeb0a513eb9edbaf840220ea65edcc968fa8064870f722eec5a`。复核后 verify 通过，没有产生适配待办，因此未运行或宣称完成 adapt。

## 能力边界

未配置命名关系规则；即使检查 exit 0，也不意味着全仓库覆盖完整。README 仅有 init 尚无正文范围，其文字修改不报 dirty 也不是语义复核证明。

真实使用已覆盖初始化、文件/目录统计、范围/link、查询以及检测真实规格移动。适配/clean/unclean、来源替换、跨项目、reset/GC/activate 尚未在本仓库实际运行；不为演练制造破坏。Skill 的其他分支明确区分契约与实践。

仓库内 skill 尚未全局安装。未提交推送、安装 hook、同步主规格或归档 change。
