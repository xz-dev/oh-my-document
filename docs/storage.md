# 存储、配置路径与发现

本文件记录当前 Rust/CLI 的持久化和本机定位边界。早期把布局、发现和 alias 全部列为候选/Q 项的文字是历史设计记录；当前行为以 `separate-range-identity-and-location` 及实际测试为准。

## 1. 数据职责

| 内容 | 权威性 | 默认位置 |
| --- | --- | --- |
| manifest、state、不可变 commit、来源版本、binding、link、登记、inbound 保护、理由 | 持久权威 | `<project-root>/.omd/` 或显式 metadata 目录 |
| 完整内容基线 | 必要时的权威恢复依据；不是 hash 可替代的数据 | metadata 下内容存储 |
| 本机 alias、project root、metadata root、remote 认可映射 | 本机配置，不是共享图的第二套权威 | 配置根 `projects.toml` |
| 查询索引 | 可删除重建 | 缓存根；按 metadata 实例隔离 |

权威记录使用结构化文本；完整原始内容可以是任意字节，不伪装成文本。SQLite/查询缓存不能保存唯一理由、唯一来源 binding 或唯一 Myers 基线。

不可变记录按 ID 单独发布；mutable state/binding 选择当前有效版本。格式版本不受支持、未知字段、损坏 identity、tip/记录不一致或旧代数据都明确拒绝并保持原字节。不做自动迁移、双格式写入或覆盖初始化。

## 2. 配置根与缓存根

优先级：

```text
OMD_CONFIG_PATH
  → 有效 XDG_CONFIG_HOME 下的 omd/
  → 平台后备配置目录

OMD_CACHE_PATH
  → 有效 XDG_CACHE_HOME 下的 omd/
  → 平台后备缓存目录
```

OMD 专用变量直接指定完整目录；空值视为未设置，相对值相对调用 cwd。显式无效/不可访问值报错，不退回其他位置。修改配置根不迁移项目 metadata，修改缓存根不改变权威记录。

`OMD_META`/`--meta` 选择 metadata；`--root` 选择项目根；`--project` 选择逻辑项目 alias；`--store` 选择登记的 store 上下文。显式选择之间不兼容时拒绝，不按方便的路径换绑。

## 3. 编码配置

编码配置使用 `format = "omd.encoding/1"`：

- 用户默认：`<config-root>/omd.toml`；
- 项目/精确文件默认：`<metadata-root>/omd.toml`。

```toml
format = "omd.encoding/1"
default_encoding = "windows-1252"

[file."docs/legacy.txt"]
encoding = "shift_jis"
```

优先级固定为：本次 `--encoding` → 已记录来源版本 → 精确文件 → 项目 → 用户 → UTF-8。配置只选择文本视图，不改原始字节、BOM 或换行；byte 模式不解码。未知/损坏且实际适用的配置在来源命令执行和业务发布前失败。

### 配置先行首次 init

真正的新项目允许默认 `<project-root>/.omd/` 中仅存在有效 `omd.toml`，随后第一次显式 `init`：

- 目录不得有 manifest、state、记录、内容、其他文件或残缺权威；
- 本机配置不得已有绑定该目标的映射；
- 只适用于默认 `.omd/`，不能把任意外置配置目录当成待初始化权威；
- init 保留 `omd.toml` 原字节，使用其编码，并遵守来源类型、执行许可和单次采集规则；
- 读取配置本身不创建身份、不执行 command、不授予普通写权限。

已有、损坏、旧格式、已绑定或含其他内容的目标 fail closed，metadata 与本机配置保持不变。

## 4. 项目根与 metadata 发现

项目根按以下顺序选择：

1. 显式 `--root`；
2. 本机登记中包含 cwd 的最深根；
3. 向上最近可识别 OMD 结构。

不使用 Git 猜项目根。相同深度多个不同登记报歧义；重复的完全相同映射不制造歧义。

metadata 按以下顺序选择：

1. 显式 `--meta`/`OMD_META`；
2. 与所选项目/store 匹配的本机映射；
3. 项目根下 `.omd/`；
4. 项目根直属子目录中唯一有效 manifest。

不递归扫描全项目。显式路径损坏、映射身份不符、映射目录丢失或同级候选多个时报告错误，不回退到另一套权威。非 init 命令不会创建缺失 metadata；逃出所选项目根的路径在创建 metadata 前拒绝。

## 5. 本机项目映射

`<config-root>/projects.toml` 保存 alias 到 `{project_root, metadata_root}` 的本机映射，并绑定实际 project_id、store_id、实例及映射修订。`omd project register/list/recognize-remote` 管理这些数据。

共享记录只保存逻辑 project/store 身份与项目相对路径。两台机器可把同一逻辑项目映射到不同根；读取当前文件时各自使用本机根。一个 project_id 可有多个 checkout/实例，缓存、观察凭据和写入目录必须按实际实例隔离，不能仅按 project_id 复用。

映射修订是物理观察依据，不进入业务 commit hash。移动目录后显式更新映射：

- 保留 project_id、store_id、commit/link ID 和不可变历史；
- 旧观察凭据失效；
- 不根据同名目录或相同 commit ID 静默换绑；
- 单独移动 metadata 时，不要求删除仍存在的源码目录，但旧 metadata 仍存在且构成另一权威时拒绝含糊修正。

可选 Git remote identity 在共享登记保存 remote 名/期望 URL，本机配置保存明确认可映射。比较保留用户名、大小写、端口、传输方式及原始字节；SSH/HTTPS 不自动等价。未配置时普通目录不需要 Git。

## 6. 移动与可写副本

移动同一个权威 store 与复制为另一个可写权威是不同操作：

- **移动：** 显式更新本机位置，保持 store_id。
- **原始复制：** 可查询历史，但业务写入和 GC 拒绝；仅登记路径不会赋予写权限。
- **激活副本：** `omd activate` 分配新 store_id，保留 project_id、既有 commit/link ID，并为全部必要外部端点完成精确保护登记。必要 peer 缺失、离线、映射错误、旧依据或锁冲突时，激活在本地发布前失败。
- **激活后：** 新 store 可在原链根下追加自己的提交；原 store 不变，其他位置原本指向原 store 的完整引用也不改指副本。

这不是同步或合并协议。两个可写 store 各有权威命名空间；OMD 不自动传送历史或处理离线分叉。

## 7. 缓存删除与重建

`omd reindex` 只从权威 published/state/records 重建派生查询索引：

- 删除 `OMD_CACHE_PATH` 后，对象链根、tip、有效范围版本、来源版本、位置和 link ID 保持；
- 重建不读取当前来源替代历史，不执行 command，不获取 Git 对象或联网；
- 不同 metadata 实例使用不同 cache key；一个实例的当前观察不能被另一个 checkout 借用；
- 篡改派生位置/类型投影不能改变不可变记录含义，读取时应拒绝不一致。

## 8. 单写者、观察凭据与发布

写入需要 `verify --json` 返回的 `data.expected`，并在写锁内核对 publication、tips、来源版本/hash、登记、peer、实例和 mapping revision。缺失必需字段是用法错误；旧值是版本冲突；真实锁冲突保持独立 typed exit。系统不自动读取新值后重试。

同一 metadata 根的锁覆盖“读取当前状态 → 核对 expected → 发布”。外部编辑器仍可改变来源文件，因此来源观察的 hash/版本/实际完整字节也必须一致。

单 store 发布使用先写不可变文件、同步、原子替换 published/state 的协议；失败点决定是否已有成员公开。跨 store 先写 inbound 保护，再发布消费者。共享权威登记和本机配置跨文件系统时不承诺分布式事务：失败结果必须如实报告实际已发布部分，不假称整体回滚。

组合操作采用逐次发布 ATOMIC 结构，不是多记录数据库事务。晚期 I/O 失败可留下 BEGIN、正文、本地 link、peer protection 等已成功成员和开放边界；JSON v2 报告成功 ID 集合、失败步骤、开放边界和 operation ID。未被 published 选择的物理临时/creator 文件不等于发布成功，也不产生额外清理承诺。

## 9. GC 与保留闭包

显式 `gc` 才清理未引用 dangling；查询、log、reset 或 reindex 不触发清理。保留闭包包含：

- 当前 tip、链根和必要前驱；
- 有效范围正文、BEGIN/END 块结构和文件快照中的子范围 tip；
- 完整来源版本、恢复 binding 和仍需的内容；
- link 创建记录、端点选定版本和 inbound/peer 保护；
- 离线但必要 consumer 的保守保护原因。

无关 peer 离线不阻塞本地操作。只有确知保护对应从未发布且无保留记录时，GC 才能收集孤儿保护。
