## ADDED Requirements

### Requirement: Cosmetic batch finishing reclassifies under the write lock

用户 SHALL 能以 `commit cosmetic <path>` 对该文件当前被过滤为结构无变化的脏范围执行批量收尾。该命令 SHALL 在取得新的调用方观察凭据并进入写锁后，对**锁内当前内容重新执行结构分类**；分类结果与用户意图确认时的过滤视图不一致的范围 MUST NOT 被续改，SHALL 如实留在报告中。收尾 MUST NOT 复用上一次 check 的分类结果作为写入依据。

通过重分类的范围 SHALL 逐条续改到新坐标，在一个 ATOMIC 块内发布；晚期 I/O 失败 SHALL 按既有部分发布契约如实报告成功成员、失败步骤、开放边界和 operation ID。分类证据（工具身份与版本、旧/新版本依据、判定）SHALL 随确认提交记录；收尾 SHALL NOT 引入第四种标记状态，确认后的范围仍是普通已确认三态。下游 link 因收尾产生的待适配责任 SHALL 保留为独立待处理项，不被批量消除。

#### Scenario: Content changed between filtering and finishing is refused
- **GIVEN** 用户以 `--difftastic` 过滤后，某同事又向同一文件加入了逻辑修改
- **WHEN** 用户运行 commit cosmetic
- **THEN** 锁内重分类发现该文件不再是结构无变化，拒绝续改并如实报告
- **AND** 不基于过期的分类视图发布任何确认

#### Scenario: A formatting storm is finished in one command
- **GIVEN** 十个范围被过滤为结构无变化，用户已逐条审完 dirty 中的真实修改
- **WHEN** 用户运行 commit cosmetic 并通过版本校验
- **THEN** 十个范围在锁下重分类后于一个 ATOMIC 块内逐条续改，证据随提交落库
- **AND** 无过滤 verify 此后返回 exit 0，下游待办如实保留

#### Scenario: Partial publication is reported honestly
- **GIVEN** 批量收尾在第七个范围发布后遭遇 I/O 失败
- **WHEN** 命令以 JSON 返回失败
- **THEN** 输出已成功的成员、失败步骤、开放边界与 operation ID
- **AND** 不假称整块已回滚，也不把失败伪装成全部完成
