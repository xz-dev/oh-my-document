# README regression scripts / README 回归脚本

These are the former monolithic README workflows, retained as maintainer checks. They exercise range/link creation, rename identity, and dirty detection with assertions; they are **not** the human introduction and do not complete adaptation.

这些是原 README 的整段自动化流程，保留给维护者检查范围/link、改名身份和变脏行为；不是人类入门教程，也不包含完整适配。

For the explanation and manual workflow, read [English tutorial](../../docs/tutorials/retry.md) / [中文教程](../../docs/tutorials/retry.zh-CN.md).

From the repository root, with Bash and Python 3:

```bash
OMD_BIN="$PWD/target/debug/omd" KEEP_DEMO=1 bash examples/regression/readme-en.sh
OMD_BIN="$PWD/target/debug/omd" KEEP_DEMO=1 bash examples/regression/readme-zh.sh
```

Build the binary first (`cargo build --locked --bin omd`) or set `OMD_BIN` to an installed version. Each script uses an isolated HOME/config/cache and prints its fixture location when `KEEP_DEMO=1`. Without that flag, its exit trap removes the fixture. Set `TMPDIR` to a writable directory if the system temporary filesystem is full. Neither script operates on the repository's `.omd/`.

The current human tutorials contain multiple explanatory command blocks instead of the old `readme-workflow` markers. Validate those blocks in order in a fresh isolated shell; see [validation notes](../../docs/tutorials/validation.md) for the checked outcomes and limitations.
