# Tutorial validation / 教程验证

## English

Run from repository root:

```bash
python3 examples/regression/check-tutorials.py
```

The checker extracts and runs the exact Bash blocks from `retry.md` and `retry.zh-CN.md` in separate temporary projects. It checks:

- file initialization and fresh `verify` evidence;
- requirement and implementation range positions;
- requirement → implementation link endpoints;
- dirty detection after changing the requirement;
- stable chain roots while creating new versions;
- a separate `commit adapt` record containing the selected link and change;
- final `verify` state and the history chain.

Current replay result: **English 13 blocks passed; Chinese 13 blocks passed**.

This is a Linux/Bash/Python 3/jq tutorial check against the selected `omd` binary (default: `target/debug/omd`). It uses isolated HOME/config/cache and does not touch the repository `.omd/`. It verifies the workflow mechanics and selected constant, not business correctness, a complete retry loop, all source types, or every platform.

The scripts in `examples/regression/` are maintainer regression material. They are intentionally separate from the human tutorial.

## 中文

在仓库根目录运行：

```bash
python3 examples/regression/check-tutorials.py
```

检查器会从 `retry.md` 和 `retry.zh-CN.md` 提取实际 Bash 区块，在两个隔离临时项目中执行，并检查：

- 文件初始化与新的 `verify` 凭据；
- 需求和实现范围的位置；
- 需求 → 实现的 link 端点；
- 修改需求后的 dirty 检测；
- 新版本提交时链根身份保持不变；
- 独立的 `commit adapt` 记录，包含选中的 link 和变化；
- 最终 `verify` 状态与历史链。

当前回放结果：**英文 13 个区块通过；中文 13 个区块通过**。

这是针对选定 `omd` 二进制的 Linux/Bash/Python 3/jq 教程检查（默认使用 `target/debug/omd`）。它使用隔离的 HOME、配置和缓存，不触碰仓库 `.omd/`。它验证工作流机械行为和示例常量，不证明业务正确性、完整重试循环、全部来源类型或所有平台。

`examples/regression/` 中的脚本是维护者回归材料，和面向人的教程分开。
