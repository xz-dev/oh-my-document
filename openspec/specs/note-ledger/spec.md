# note-ledger Specification

## Purpose

定义 note 作为独立 append-only commit 链对象（取代旧平面 notes/<id>.toml）：每条 note 以 init commit 为根，patch/delete 追加不覆盖，修订序按链不按墙钟，旧平文件格式一律拒绝不迁移。

## Requirements


### Requirement: Notes form an append-only linear commit chain with their own root

note SHALL 从平面文件（`notes/<id>.toml` + seq）升级为独立线性 commit 链：每条 note 是以 init commit 为根的链，`patch`/`delete` 以新 commit 追加（携带 `previous_id`），MUST NOT 原地覆盖。note 链 SHALL 与 file/range、audit 链共用同一套 commit 记录与链式实现。修订顺序 SHALL 继续跟随发布序列而非墙钟——时钟回拨 MUST NOT 重排修订。

#### Scenario: A note patch appends a chain commit
- **GIVEN** note N 的链上已有 add commit n1
- **WHEN** 用户执行 patch
- **THEN** 追加 n2（previous 指向 n1），n1 保留可查
- **AND** note 的修订历史可以从链上完整走回

#### Scenario: Publication order still beats the clock
- **GIVEN** 两次 patch 的墙钟时间戳因时钟回拨而乱序
- **WHEN** 列出该 note 的修订历史
- **THEN** 顺序仍按链（previous_id）即发布序列呈现
- **AND** 不按时间戳重排

### Requirement: The old flat note format is rejected, not migrated

store 中存在的旧格式 `notes/<id>.toml` 平面文件 SHALL 被明确拒绝：读命令遇到时 SHALL 报错指出旧格式不被支持，MUST NOT 静默读取、转换、或当作新链数据解释；写命令 MUST NOT 写出旧格式。旧数据保留原字节不动；需要保留其中信息的所有者 SHALL 手动重建为链式 note。不存在自动迁移层、双格式读写或格式嗅探回退。

#### Scenario: A stale flat note file fails loudly
- **GIVEN** store 的 `notes/` 目录存在旧格式平面文件
- **WHEN** 用户运行任何会读取该 note 集合的命令
- **THEN** 命令报错说明旧格式被拒绝，指出文件路径
- **AND** 该文件内容不被解释为新链数据，也不被改写

#### Scenario: No silent fallback path exists
- **WHEN** 新写入一个 note
- **THEN** 只产生链式 commit 记录
- **AND** 系统不存在"写入时回落到平面格式"的任何路径
