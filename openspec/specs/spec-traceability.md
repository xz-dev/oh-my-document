
## separate-range-identity-and-location

| 规格条目 | 实现 | 验证 |
|---|---|---|
| 对象身份=链根 commit | `relations/identity.rs` `Ref`/`resolve_ref`/`commit_to_node` | `tests/identity.rs`、`tests/guards.rs` |
| 结构化来源字段 | `--source-type`+per-kind flags、`--project`、`--source-project`、`--git-remote`/`--git-remote-url` | `tests/command_source.rs`、`tests/git_source.rs`、`tests/replace_source.rs` |
| 本机映射 | `sources/projects.rs` `projects.toml` + `omd project register/list` | `cross_store::local_mapping_resolves_meta_from_subdir` |
| 跨 store 副本独立链 | activate 重发 store_id，旧 commit 保留 | `cross_store::copy_then_continue_independent_chain` |
| 旧格式拒绝 | `State.format` 三态（v1 拒/v2/缺=v1） | `identity::legacy_store_refused_untouched` |
| 坐标端点拒绝 | `file@span`/`peer:…@span` 非端点 | `guards` endpoint 测试 |
| version record missing 修复 | marker/link `content_ref="empty"` | `bdd` verify 场景 |
