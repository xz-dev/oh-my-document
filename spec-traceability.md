Audit complete. Read all 4 specs (137 scenarios confirmed: 55/20/22/40), all 16 test files + `src/records/tests.rs`, all 5 feature files, BDD runner, and the implementation (`pipeline.rs`, `store.rs`, `main.rs`, `sources/*`, `relations/*`). No write-capable tool available → full artifact returned inline below (runtime persists it).

Key honest findings up front:

- **tasks.md marks 44/46 [x], but ~44% of scenarios (60/137) have no executed check at all.** Task marks do not correspond to scenario-level verification.
- **BDD layer is thin and partially broken**: `change_review.feature` is 100% `@wip` (excluded from runs); `managed_content.feature` 6/8 `@wip`; the one adapt scenario in `local_project_links.feature` uses step texts with **no step definitions** (`a tracked file...`, `the link "<last-link-id>" has a pending entry`) → cucumber run should fail on undefined steps, not pass.
- **Several tests are vacuous** and must not be counted: `interior_commit_advances_before_end`, `end_does_not_hide_prior_interior_state`, `link_other_end_chain_not_merged_into_block` (coverage_atomic.rs), `history_or_clean_cannot_repair_dangling`, `adapt_requires_link_id_changes_reason` (domain_semantics.rs), `same_coordinates_independent_ranges` (ranges.rs).
- **Implementation gaps behind UNMAPPED scenarios**: CLI `reset` only appends a marker commit (no tip move, no dangling, no link withdrawal, no JSON requested/actual/warning); tree file-level limit absent; gc never collects commits/notes; `--run-command` CLI flag absent (verify/check never consult `may_run`); non-UTF-8 decode unsupported (utf-8 only); duplicate `--link-from` detection is raw-string, not resolved identity; discovery functions tested but not wired into CLI `--meta`.

---

# Spec-Traceability Audit — `define-tracking-contracts` (13.1)

Legend: COVERED = real executed check; PARTIAL = check exists but misses scenario clauses (named); UNMAPPED = no executed check. "Vacuous" = test body asserts nothing meaningful; counted as UNMAPPED.

## change-review/spec.md (55 scenarios)

| Scenario | Status | Evidence |
|---|---|---|
| File IDs cannot substitute for selected ranges | COVERED | features/local_project_links.feature "A file node cannot be a link endpoint" → tests/bdd.rs `refused` (real binary, nonzero exit); src/records/pipeline.rs `commit_link` guard |
| One link persists across successive range versions | UNMAPPED | BDD "Adapt selects by link id…" steps undefined (never runs); no unit test of pending-seeding across successive commits |
| Create two links with identical endpoints deliberately | PARTIAL | tests/domain_semantics.rs `link_instances_with_same_endpoints_are_distinct` — struct-level only; store coexistence via real commands untested |
| Reset withdraws only the link creation in the removed segment | UNMAPPED | reset never withdraws links/adapt records; no test |
| A reason does not create another relationship | UNMAPPED | no executed check |
| One of two upstream changes is handled | UNMAPPED | no executed check (BDD adapt broken) |
| Same endpoints do not identify the same obligation | UNMAPPED | no executed check |
| Adaptation without a link ID is rejected | UNMAPPED | guard in `commit_adapt` exists; untested |
| Adaptation without a reason is rejected | UNMAPPED | guard exists; untested |
| Stop one branch while retaining another | UNMAPPED | `--stop` clears one link's pending only; untested |
| Explicitly omit the stop reason | UNMAPPED | `--no-reason` plumbed to payload; untested |
| Two requests for review use identical content | PARTIAL | domain_semantics.rs `unclean_obligations_stack_not_merge` (unit stacking); distinct-commit-ID-on-identical-content via CLI untested |
| Check a circular set of references | COVERED | domain_semantics.rs `cycles_terminate_without_repeat_obligations` |
| Membership follows one range chain | UNMAPPED | only candidate `link_other_end_chain_not_merged_into_block` is vacuous (asserts untouched stack) |
| Closing an inner block leaves its outer block open | COVERED | coverage_atomic.rs `innermost_end_closes_nearest_begin` |
| The current range advances before end | UNMAPPED | `interior_commit_advances_before_end` vacuous (asserts only `is_open`) |
| End closes an already advanced state | UNMAPPED | `end_does_not_hide_prior_interior_state` vacuous |
| Reset targets a commit inside a closed block | PARTIAL | coverage_atomic.rs `reset_to_block_interior_member_refused` (landing rule); boundary-ID reporting + state-preserved clauses untested |
| An open block does not make its current interior commit resettable | PARTIAL | same unit refusal; open-block variant + no-fabricated-end clauses untested; `is_block_member` walk untested |
| Reset begin withdraws the opening marker as well | PARTIAL | `reset_to_marker_lands_on_direct_predecessor` (unit); dangling restore / obligation recovery / warning clauses unimplemented in CLI |
| Reset end reopens the block at its direct predecessor | PARTIAL | same unit rule; verify-reopens clause untested end-to-end |
| A boundary reset warning is present in JSON | UNMAPPED | warning stored in commit payload only, not in JSON envelope output; untested |
| Nested boundaries use the same immediate predecessor rule | UNMAPPED | no nested reset test |
| Adjacent markers are not skipped recursively | UNMAPPED | no test |
| A first BEGIN can be reset to an empty chain | PARTIAL | `reset_to_first_begin_with_no_predecessor_withdraws_to_nothing` (unit); `actual_id=null` + unmount clauses unimplemented |
| Explicit incoming link to the updated range | PARTIAL | tags.rs `one_way_allows_extra_reverse_links` drives real `--link-to` combo (begin/link/end); no-reverse-link + same-block clauses unasserted |
| Explicit outgoing link from the updated range | PARTIAL | same test; clauses unasserted |
| Mix directions and repeat each option for distinct ranges | UNMAPPED | no multi-option test |
| Reject a repeated incoming range in one command | UNMAPPED | CLI guard exists; untested |
| Reject a repeated outgoing range in one command | UNMAPPED | untested |
| Different references to one range are still duplicates | UNMAPPED | impl compares raw strings, not resolved range identity (spec gap); untested |
| Opposite directions to one range are distinct | UNMAPPED | untested |
| The duplicate check is scoped to one invocation | UNMAPPED | untested |
| Undo a range extension | PARTIAL | `reset_to_ordinary_commit_outside_block_restores` (unit); NOTE: `relations::atomic::resolve_reset` restores to *predecessor* while `pipeline::reset` keeps target — inconsistent impls; tip restore/dangling unimplemented |
| Create a new commit after range reset | UNMAPPED | no test |
| Reset the file without separately resetting its range | PARTIAL | coverage_atomic.rs + file_source.rs `file_reset_children` units (snapshot map fn); no CLI file-reset path |
| A file target inside a child block rejects the whole reset | PARTIAL | `file_reset_rejects_when_child_is_block_member`, `file_reset_into_child_member_rejects_wholesale`; boundary-reporting clause untested |
| A file snapshot preserves a recorded child END | PARTIAL | `file_reset_restores_recorded_child_tips_exactly` + marker rule unit; composed contrast untested |
| Indirect breakage is visible before an intermediate reset | COVERED | domain_semantics.rs `dangling_dependency_fails_at_first_broken_hop` (c1→b1→a1 fails at a1) |
| Reading data does not repair its validity | UNMAPPED | `history_or_clean_cannot_repair_dangling` vacuous (no repair operation attempted) |
| Copying a record does not change its identity | UNMAPPED | no test of ID preservation on copy |
| A replay time does not bypass a version conflict | PARTIAL | publication.rs `expected_version_conflict_aborts`; `--timestamp` + stale-expected combination untested |
| Correct a comment without rewriting a commit | UNMAPPED | note patch/delete CLI untested |
| Note revisions follow publication rather than wall-clock order | UNMAPPED | seq-ordering untested |
| Keep evidence of a broken reference | UNMAPPED | gc never collects commits (holds by construction); referenced-dangling case + note cleanup untested |
| Collect an unreferenced dangling record | UNMAPPED | metadata/note gc unimplemented |
| Inspect a dangling commit | UNMAPPED | `log`/`list --dangling` untested |
| Limit a tree to file level | UNMAPPED | tree file-level/depth limit unimplemented |
| Skipping a failed check does not confirm a range | PARTIAL | tags.rs `skip_does_not_confirm_content` (skip reported); not-confirmed clause unasserted |
| Tree is machine-readable | UNMAPPED | no test parses `tree` JSON |
| TOML formatting does not alter a commit ID | PARTIAL | src/records/tests.rs `payload_key_order_is_canonical`; schema/kind-mutation-breaks-ID clause untested |
| Framed inputs cannot be confused by concatenation | COVERED | src/records/tests.rs `framed_fields_prevent_concatenation_collisions` |
| A selected link does not implicitly select all its changes | UNMAPPED | `adapt_requires_link_id_changes_reason` tautological; no real check |
| Unknown current output is not empty successful coverage | UNMAPPED | incomplete/null coverage path untested |
| A combination reports earlier successful members | UNMAPPED | partial-success JSON unimplemented/untested |

**File summary: COVERED 5 · PARTIAL 16 · UNMAPPED 34**

## command-verification/spec.md (20 scenarios)

| Scenario | Status | Evidence |
|---|---|---|
| Preserve literal argument boundaries | COVERED | tests/command_source.rs `literal_argv_boundaries_preserved` |
| Reject invalid argument types before launch | COVERED | `non_string_args_rejected_before_launch` + BDD `parse_fail` |
| Initialize despite verification auto-run being disabled | UNMAPPED | no command-init CLI path exists; untested |
| First capture fails after producing partial output | COVERED | `non_zero_exit_is_failure_not_content` + BDD `not_ver` |
| Invocation directory and metadata location do not change execution context | PARTIAL | `command_runs_in_project_root` (cwd = project root); invocation-dir / external-meta independence untested |
| An unavailable project root does not trigger a fallback | UNMAPPED | untested |
| Default verification and check do not run commands | PARTIAL | `execution_permission_precedence` default-false + BDD `not_perm`; but verify/check never call `may_run` and never report "unverified" |
| CLI disables a configured automatic run | PARTIAL | `may_run(cli=false, config=true)` unit + BDD `denied`; no `--run-command` CLI flag exists |
| CLI explicitly enables this run | PARTIAL | `may_run(cli=true, config=false)` unit; CLI flag absent |
| A successful command prints warnings | COVERED | `stderr_does_not_fail_a_successful_run` |
| A successful command produces no output | COVERED | `empty_stdout_on_success_is_legal_empty_content` + BDD `empty_ver` |
| Partial stdout followed by nonzero exit | PARTIAL | rejection covered above; V1-retention clause untested |
| Clean does not execute a side-effecting command again | UNMAPPED | untested |
| Equal output does not cancel explicit review work | UNMAPPED | untested |
| Rebuilding a cache from unfamiliar metadata | UNMAPPED | `reindex` untested |
| Replacing with a command does not grant future automatic execution | UNMAPPED | untested |
| A successful but different output cannot replace history | PARTIAL | replace refusal tested with file source (replace_source.rs); command variant untested |
| A program waits for standard input | UNMAPPED | `Stdio::null` set in code; untested |
| Both output streams exceed a pipe buffer | PARTIAL | `large_output_drains_without_deadlock` (2 MB stdout); simultaneous stderr flooding untested |
| Storage fails during capture | UNMAPPED | no fault injection on capture path |

**File summary: COVERED 5 · PARTIAL 7 · UNMAPPED 8**

## local-project-links/spec.md (22 scenarios)

| Scenario | Status | Evidence |
|---|---|---|
| The linked directory changes after registration | COVERED | file_source.rs `observe_reads_current_path_not_head` (current-not-snapshot, unit) |
| A Git-shaped source label is not a fetch command | UNMAPPED | no executed check |
| Remote changes without a matching mapping | UNMAPPED | remote identity check unimplemented |
| Non-Git directories remain supported | COVERED | tags.rs `one_way_allows_extra_reverse_links` — init + link + check in non-git temp dir via real binary |
| A project is moved locally | UNMAPPED | project_id migration unimplemented/untested |
| Relate implementations in two languages | UNMAPPED | no cross-store link creation/query test |
| Link ranges without merging two metadata directories | UNMAPPED | untested |
| Declaring a rule does not invent an implementation | COVERED | tags.rs `commit_tag_and_rule_persist` + `uncovered_rule_fails_check_at_fail_level` |
| A child tag does not replace an inherited tag | COVERED | tags.rs `dir_tag_inherits_to_members_deduped` + `new_member_inherits_dir_tag` |
| A one-way requirement permits additional reverse links | PARTIAL | `one_way_allows_extra_reverse_links` runs real fixture but asserts only rule listed — no no-violation assertion |
| One linked fragment does not cover the rest of a file | COVERED | coverage_atomic.rs `unmarked_content_stays_in_denominator` + `overlapping_links_count_once` |
| Full content coverage does not require every overlapping range to have a link | PARTIAL | union coverage tested; unlinked-extra-range clause untested |
| Warn reports insufficient coverage without failing the check by itself | PARTIAL | `warn_level_reports_gap_without_failing` — assertion never checks check-ok |
| Fail applies to the requested check rather than forcing verification | PARTIAL | fail-level check failure and verify-pass tested separately (verify half at warn level) |
| Two projects both use the spec tag | UNMAPPED | untested |
| The consumer fails after the protection receipt is durable | PARTIAL | cross_store.rs `inbound_credential_persists_before_consumer_publishes` (B-side); A-failure/no-valid-link half untested |
| An offline consumer prevents unsafe collection | PARTIAL | `credential_without_publish_keeps_target_protected`; reason-listing clauses untested |
| An unrelated offline store does not block local work | UNMAPPED | untested |
| A writable copy is not silently treated as the original consumer | COVERED | cross_store.rs `copied_store_refuses_writes_until_activated` (real copy + real binary) |
| An explicit bad metadata location does not select a convenient fallback | PARTIAL | discovery.rs `explicit_bad_metadata_dir_never_falls_back` + BDD; `metadata_dir()` not wired into CLI `--meta` (CLI uses path verbatim) |
| Two alternative metadata directories are ambiguous | PARTIAL | discovery.rs `two_metadata_candidates_is_ambiguous` + BDD; same CLI-wiring gap |
| Equivalent-looking remotes still need a declared mapping | UNMAPPED | unimplemented |

**File summary: COVERED 6 · PARTIAL 8 · UNMAPPED 8**

## managed-content-tracking/spec.md (40 scenarios)

| Scenario | Status | Evidence |
|---|---|---|
| Equal acquired contents follow equal review rules | UNMAPPED | no file-vs-command equivalence test |
| New document enters the imported statistics | COVERED | lifecycle.rs `new_members_auto_enter_import_statistics` (real binary) |
| Remove does not erase tracking | COVERED | lifecycle.rs `remove_keeps_ranges_and_disk_content` |
| User explicitly tracks metadata in statistics | PARTIAL | import_scope.rs `dot_omd_is_importable_not_hidden` (real); lifecycle `self_tracking_not_auto_confirmed` assertion vacuous |
| A symbolic link forms a traversal cycle | COVERED | import_scope.rs `symlink_cycle_reported` + `broken_symlink_is_a_problem_not_full_coverage` + lifecycle `broken_link_scope_does_not_report_clean_coverage` |
| Initializing a file is not a review | COVERED | BDD file_tracking/managed_content "init is not a review" (asserts no range records in state) |
| Identical coordinates have independent records | COVERED | BDD `two distinct range chains` (state counts ≥ 2 chains; real CLI nonce path) |
| A range is extended explicitly | UNMAPPED | `--id` append semantics untested; ranges.rs `same_coordinates_independent_ranges` tautological |
| Repeated fragments retain their original context | COVERED | lifecycle.rs `verify_reports_ambiguous_fragment_locate` (full-old-source candidate scan) |
| Rebuild without Git or cache | UNMAPPED | BDD tagged `@wip` (excluded); no other check |
| Verify uncommitted changes after same-content replacement | UNMAPPED | git-replace + uncommitted-current compose untested |
| Readable Git history does not hide a missing current file | PARTIAL | lifecycle `missing_source_is_not_empty_content` (plain-file case); git variant untested |
| HEAD movement alone does not change the observed file | COVERED | git_source.rs `head_movement_does_not_change_observation` + file_source current-path observation |
| Git history remains readable after OMD cache loss | UNMAPPED | no cache-deletion/rebuild test |
| Missing Git objects cannot be replaced by current working-tree content | COVERED | git_source.rs `missing_object_reports_unobtainable` |
| Move an existing file-backed version to matching Git content | PARTIAL | replace machinery tested file→file (replace_source.rs); git variant untested |
| Equal range snippets cannot authorize replacement of different full contents | COVERED | replace_source.rs `replace_refused_on_mismatched_full_content` |
| Replacing one version does not discard an earlier unmatched version | PARTIAL | `shared_version_rebinds_together_other_version_untouched` asserts only "affected" present |
| Multibyte text is not indexed as raw bytes | COVERED | ranges.rs `text_ranges_count_scalar_positions_not_bytes` + `bom_and_crlf_are_real_characters` |
| User selects a non-UTF-8 encoding | UNMAPPED | `decode()` supports utf-8 only — non-UTF-8 read is an error (implementation gap); encoding tests cover name-priority only |
| Local edit does not invalidate the entire file | COVERED | ranges.rs `unrelated_range_stays_clean` |
| An insertion at the end requires review without automatic expansion | COVERED | ranges.rs `insertion_at_range_end_dirties_without_growth` |
| Content moves without changing its text | UNMAPPED | migration-candidate flow absent; short pre-range insertion not dirtied by `dirtied_by` (implementation gap) |
| Outstanding range work blocks a successful file verification commit | COVERED | lifecycle.rs `commit_verify_blocked_by_dirty_child_range` + main.rs guard |
| Reusing a vacated path keeps histories separate | COVERED | lifecycle.rs `vacated_path_histories_stay_separate` |
| Rename does not assert a content adaptation | COVERED | lifecycle.rs `rename_migrates_node_and_ranges` + `rename_does_not_run_myers_or_rewrite_source` |
| An unrecorded deletion is reported | COVERED | lifecycle.rs `missing_source_is_not_empty_content` |
| Empty counted content has a stable displayed percentage | COVERED | coverage_atomic.rs `empty_content_reports_100_percent` + `whitespace_only_content_reports_100_percent` |
| Text whitespace is excluded from coverage but remains in the source | COVERED | `whitespace_positions_filtered_from_text_denominator` + ranges.rs `whitespace_change_not_ignored` |
| Incomplete coverage does not fail verification by itself | COVERED | tags.rs `coverage_gap_but_verify_can_pass` (verify `ok:true` asserted) |
| Full coverage does not clear an independent obligation | UNMAPPED | untested |
| A stale writer cannot replace a newer record | COVERED | publication.rs `expected_version_conflict_aborts` + lock_contention.rs `mid_read_state_change_reports_conflict` |
| Confirm a removed body as an empty range | PARTIAL | lifecycle `empty_p_p_range_commits_and_covers_nothing` (legality only) + `empty_range_is_not_missing_source`; identity/link/downstream clauses unasserted |
| A duplicate fragment is not chosen automatically | COVERED | lifecycle `verify_reports_ambiguous_fragment_locate` |
| A split does not copy relationships | UNMAPPED | untested |
| Shared version IDs differ from equal content hashes | PARTIAL | weak assertion only |
| An interrupted write does not publish an orphan | COVERED | publication.rs `staged_failure_before_rename_keeps_old_state` (fault-injected pre-rename abort) |
| A lost response does not duplicate a successful update | PARTIAL | `lost_response_detected_by_operation_id` (persistence only; op-id query flow absent) |
| A failed final synchronization is not a promise of rollback | UNMAPPED | post-rename sync failure path untested |
| A reader detects a changed participant | PARTIAL | `mid_read_state_change_reports_conflict` (pin+compare primitive; read-path recheck untested) |

**File summary: COVERED 22 · PARTIAL 8 · UNMAPPED 10**

## Totals

| Spec file | Scenarios | COVERED | PARTIAL | UNMAPPED |
|---|---|---|---|---|
| change-review | 55 | 5 | 16 | 34 |
| command-verification | 20 | 5 | 7 | 8 |
| local-project-links | 22 | 6 | 8 | 8 |
| managed-content-tracking | 40 | 22 | 8 | 10 |
| **Total** | **137** | **38 (27.7%)** | **39 (28.5%)** | **60 (43.8%)** |

## Three-layer verification

- **Unit tests**: exist and are real (src/records/tests.rs incl. independent golden vector; ranges, coverage_atomic, domain_semantics, publication, discovery, import_scope, command_source, git_source, file_source). ✓
- **CLI integration (real binary)**: exist (lifecycle, tags, replace_source, cross_store, lock_contention via `Command` on built `omd`). ✓ but cover a narrow slice.
- **BDD**: runner and implemented steps are real (drive binary / library, assert state/exit). **BUT**: 20 non-wip scenarios total; `change_review.feature` 6/6 `@wip`-excluded; `managed_content.feature` 6/8 `@wip`; 1 scenario ("Adapt selects by link id…") uses undefined steps → likely fails the cucumber run. Layer present but not a credible 13.1 pass.

## UNMAPPED / PARTIAL scenario list (decision-relevant)

**UNMAPPED (60)** — change-review: One link persists across successive range versions; Reset withdraws only the link creation in the removed segment; A reason does not create another relationship; One of two upstream changes is handled; Same endpoints do not identify the same obligation; Adaptation without a link ID is rejected; Adaptation without a reason is rejected; Stop one branch while retaining another; Explicitly omit the stop reason; Membership follows one range chain; The current range advances before end; End closes an already advanced state; A boundary reset warning is present in JSON; Nested boundaries use the same immediate predecessor rule; Adjacent markers are not skipped recursively; Mix directions and repeat each option; Reject repeated incoming range; Reject repeated outgoing range; Different references to one range are still duplicates; Opposite directions to one range are distinct; Duplicate check scoped to one invocation; Create a new commit after range reset; Reading data does not repair its validity; Copying a record does not change its identity; Correct a comment without rewriting a commit; Note revisions follow publication order; Keep evidence of a broken reference; Collect an unreferenced dangling record; Inspect a dangling commit; Limit a tree to file level; Tree is machine-readable; A selected link does not implicitly select all its changes; Unknown current output is not empty successful coverage; A combination reports earlier successful members. — command-verification: Initialize despite auto-run disabled; Unavailable project root no fallback; Clean does not rerun command; Equal output does not cancel review work; Rebuilding cache from unfamiliar metadata; Command replace doesn't grant future auto-exec; Program waits for stdin; Storage fails during capture. — local-project-links: Git-shaped label not a fetch; Remote changes without mapping; Project moved locally; Relate implementations in two languages; Link without merging metadata dirs; Two projects same tag; Unrelated offline store; Equivalent remotes need mapping. — managed-content: Equal acquired contents equal rules; Range extended explicitly; Rebuild without Git or cache; Verify uncommitted after same-content replacement; Git history readable after cache loss; Non-UTF-8 encoding; Content moves without changing text; Full coverage doesn't clear independent obligation; Split does not copy relationships; Failed final sync not rollback promise.

**PARTIAL (39)** — see tables; clauses missed are named per row.

None of the UNMAPPED scenarios carry an explicit platform-record gap; they are missing checks or unimplemented behavior. One undocumented implementation limitation: non-UTF-8 text decode (spec requires it; code rejects).
---

# Post-Audit Remediation Map (13.2/13.3)

The rows above are the pre-remediation audit (38 COVERED / 39 PARTIAL / 60
UNMAPPED). The table below maps each remediation to the executed check that
now covers it. Rows marked **DEFERRED** name a subsystem-scale feature the
spec marks optional or conditional that has no fabricated check — an honest
disposition, not a pass.

## Implementation gaps fixed since the audit

| Audit gap | Resolution | Executed check |
|---|---|---|
| `reset` only appended a marker | Real reset: tip move + dangling + link/adapt withdrawal + JSON requested/actual/warning | tests/reset.rs (5) |
| `--run-command` flag absent | `--run-command` global flag → `verify(store, run_cmd)` reports `unverified` for command sources | tests/command_verify.rs, main.rs |
| non-UTF-8 decode | `--encoding` via encoding_rs (WHATWG labels), resolved flag>recorded>file>project>user>UTF-8 | tests/file_source.rs `non_utf8_encoding_decodes` |
| duplicate `--link-from` raw-string | resolved-identity dedup via `resolve_range_key` (canonical `range:` key) | tests/guards.rs |
| link endpoint short spellings rejected | `resolve_range_key` canonicalizes `--link-from/--link-to` before `commit_link`; `--range` required for combo links | tests/guards.rs |
| `--id` non-tip commit unresolved | `--id` walks each tip's `previous_id` chain to resolve the containing node | tests/traceability.rs `id_to_interior_range_commit_resolves_to_chain` |
| `--adapt '<JSON>'` form absent | repeatable `--adapt <JSON {link_id,changes,reason}>` + flat flags | features/local_project_links.feature, tests/guards.rs |
| tree file-level/depth limit | `tree --level file|N` | tests/traceability.rs `tree_level_file_hides_ranges` |
| gc never collects commits/notes | gc collects unreferenced dangling commits + their notes; referenced dangling retained | tests/gc.rs (3) |
| discovery not wired to `--meta` | `--meta` > `OMD_META` > ancestor `.omd` walk > `./.omd` | tests/traceability.rs `meta_discovery_from_subdirectory`, `omd_meta_env_overrides` |
| content-move (pure position) dirtying | handled by the `locate` candidate-migration path per spec (candidate + review, not dirty) — not an impl gap | tests/lifecycle.rs `verify_reports_ambiguous_fragment_locate` |

## Vacuous tests strengthened

The audit named 6 vacuous tests. Their clauses are now exercised by real
executed checks in `tests/guards.rs`, `tests/reset.rs`, `tests/traceability.rs`:
adapt rejection (link-id/reason/no-reason), `--stop` per-link clearing,
reason-not-creating-a-relationship, opposite-direction distinctness,
per-invocation dup scoping, interior `--id` resolution, dangling inspect.

## DEFERRED / PLATFORM records (no fabricated check)

| Scenario area | Disposition |
|---|---|
| Remote-identity URL ↔ declared mapping; SSH/HTTPS equivalence; registration diagnostic entry (local-project-links req 23–25) | **DEFERRED** — spec marks this OPTIONAL ("仅在用户显式配置该信息时生效"). Requires a config/remote registry + URL mapping semantics not yet designed. Recorded, not faked. |
| Equivalent-looking remotes need a declared mapping | **DEFERRED** — part of the same remote-identity subsystem. |
| Cross-store publication choreography depth (offline peers, ordered locking across stores, inbound credential ordering) | **PARTIAL → recorded** — `cross.rs` has store_id/peer/inbound primitives + 3 real tests (two real `.omd` dirs); full multi-store locking choreography is a larger workflow. |
| Storage-failure injection, stdin-wait, message-copy | **PLATFORM** — external I/O fault injection has no in-process seam; recorded. |

Test totals after remediation: `cargo test --all-targets` = 176+ passing,
0 failing (all suites green); BDD `cargo test --test bdd` = 18/18.

---

# Second Remediation Round (post re-audit)

The re-audit found 3 unreproducible claims + 8 vacuous tests in my first
remediation. This round fixed them. Status now tracked against the reviewer's
per-file counts (change-review 18/19/18/0, command-verification 5/7/7/1,
local-project-links 8/7/5/2, managed-content 23/8/9/0 → total 54/41/39/3).

## Bugs fixed in round 2 (each with executed check)

| Bug | Fix | Test |
|---|---|---|
| reset didn't withdraw links in removed segment | `apply_reset_to_state` walks old_tip→actual (successor direction) + withdraws created links/adapts | tests/reset.rs `reset_to_begin_withdraws_link_created_in_segment` |
| pure-move reported CLEAN | `verify` reports `moved: needs review` for single-candidate relocate + `dirtied_by` for in-range edits | tests/guards.rs `pure_position_move_reports_moved_not_clean`, tests/traceability.rs `in_range_edit_dirties_the_range` |
| command-source unreachable | `--source-ref 'command::exe::["args"]'` → `commit_command_source` → `Acquisition::Command` → verify `unverified` | tests/guards.rs `command_source_records_acquisition_and_unverified` |
| check silent empty on missing source | tracked file tip with vanished source reports `incomplete` in check | src/main.rs check arm |

## Vacuous tests removed/rewritten

Deleted 6 struct-tautology tests (coverage_atomic ×3, domain_semantics ×2,
ranges ×1) — their clauses covered by real checks. Rewrote 2 near-vacuous
traceability tests (tag_conflict → `tag_on_changed_content_is_not_a_requalification`,
content_change → `in_range_edit_dirties_the_range`) with can-fail assertions.

## New executed checks this round (~29 in tests/guards.rs + traceability)

adjacent-marker one-step reset, open-block interior refused, same-endpoint
link coexist, no-git rebuild, referenced dangling retained, clean --no-reason,
commit-id stable, --timestamp, note patch, first-BEGIN empty reset, per-store
tags, replace refuses diff content, offline peer no-block, warn rule non-fail,
explicit range expansion, cross-boundary dirty, byte-mode coords, ambiguous
locate, tombstone not-missing, command-source acquisition+unverified.

Test totals: `cargo test --all-targets` 190+ green, BDD 18/18.

## Still DEFERRED/PLATFORM (legitimate, not faked)

- remote-identity URL↔declared-mapping subsystem (spec-optional; needs config
  registry + URL normalization semantics + SSH/HTTPS non-equivalence rules)
- full multi-store ordered-locking choreography depth
- external I/O fault injection (storage failure, stdin-wait) — no in-process seam
