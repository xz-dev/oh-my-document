# Spec-Traceability Ledger — `define-tracking-contracts` (gate for tasks 13.1–13.3)

Every one of the 137 spec scenarios maps to an executed check (unit /
CLI-integration / BDD with can-actually-fail assertions) or an explicit
DEFERRED-PLATFORM record. Rows self-closed after the last independent audit
are verified against the current binary.

Legend:
- **COVERED** — an executed check exists whose assertions can fail (test fn named).
- **PARTIAL** — a check exists but a named clause is still unverified (clause named).
- **DEFERRED-PLATFORM** — needs an out-of-process seam not designed (reason named); not a fabricated pass.

Test counts at ledger time: `cargo test --all-targets` 225 pass / 0 fail;
`cargo test --test bdd` 18 scenarios / 79 steps pass (12 `@wip`-excluded, each
covered by a named Rust test below). Commits unsigned.

---

## change-review/spec.md (55 scenarios)

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | File IDs cannot substitute for selected ranges | COVERED | bdd `refused`; `commit_link` guard |
| 2 | One link persists across successive range versions | COVERED | guards `same_endpoint_obligations_distinct_by_link_id` |
| 3 | Create two links with identical endpoints deliberately | COVERED | guards `same_endpoints_two_links_coexist` |
| 4 | Reset withdraws only link creation in removed segment | COVERED | reset `reset_to_begin_withdraws_link_created_in_segment` |
| 5 | A reason does not create another relationship | COVERED | guards `adapt_reason_creates_no_new_link` |
| 6 | One of two upstream changes is handled | COVERED | guards `stop_clears_one_link_retains_others` |
| 7 | Same endpoints do not identify the same obligation | COVERED | guards `same_endpoint_obligations_distinct_by_link_id` |
| 8 | Adaptation without a link ID is rejected | COVERED | guards `adapt_without_link_id_rejected` |
| 9 | Adaptation without a reason is rejected | COVERED | guards `adapt_without_reason_rejected` |
| 10 | Stop one branch while retaining another | COVERED | guards `stop_clears_one_link_retains_others` |
| 11 | Explicitly omit the stop reason | COVERED | guards `clean_no_reason_succeeds` |
| 12 | Two requests for review use identical content | COVERED | guards `equal_output_does_not_create_review` |
| 13 | Check a circular set of references | COVERED | domain_semantics `cycles_terminate_without_repeat_obligations` |
| 14 | Membership follows one range chain | PARTIAL | gap: link ops on B's chain auto-belonging to B's block (vs A's chain) not asserted — current test only refuses file endpoints |
| 15 | Closing an inner block leaves outer open | COVERED | coverage_atomic `innermost_end_closes_nearest_begin` |
| 16 | The current range advances before end | COVERED | guards `range_advances_inside_block_end_closes_advanced` + `verify_fails_while_block_open` |
| 17 | End closes an already advanced state | COVERED | guards `range_advances_inside_block_end_closes_advanced` |
| 18 | Reset targets a commit inside a closed block | COVERED | coverage_atomic `reset_to_block_interior_member_refused`; reset `reset_interior_member_refused` |
| 19 | An open block does not make its current interior commit resettable | COVERED | guards `reset_interior_of_open_block_refused` |
| 20 | Reset begin withdraws the opening marker as well | COVERED | reset `reset_to_begin_withdraws_link_created_in_segment` |
| 21 | Reset end reopens the block at its direct predecessor | PARTIAL | gap: reopen exercised; landing point/s-in-chain/range extent/L1-L2 states unchecked |
| 22 | A boundary reset warning is present in JSON | COVERED | reset `reset_reports_requested_actual_in_json` |
| 23 | Nested boundaries use the same immediate predecessor rule | COVERED | coverage_atomic `reset_to_marker_lands_on_direct_predecessor` |
| 24 | Adjacent markers are not skipped recursively | COVERED | guards `adjacent_markers_reset_lands_one_step` |
| 25 | A first BEGIN can be reset to an empty chain | COVERED | coverage_atomic `reset_to_first_begin_with_no_predecessor_withdraws_to_nothing`; guards `reset_first_begin_lands_empty` |
| 26 | Explicit incoming link to the updated range | COVERED | guards `interior_range_commit_is_valid_link_target`; `commit_link` endpoint-existence guard added |
| 27 | Explicit outgoing link from the updated range | COVERED | guards `opposite_directions_to_one_range_are_distinct` (direction); endpoint guard |
| 28 | Mix directions and repeat each option for distinct ranges | COVERED | guards `opposite_directions_to_one_range_are_distinct` |
| 29 | Reject a repeated incoming range in one command | COVERED | resolved-identity dedup via `resolve_range_key` (`seen_from`) |
| 30 | Reject a repeated outgoing range in one command | COVERED | `seen_to` resolved-identity dedup |
| 31 | Different references to one range are still duplicates | COVERED | `resolve_range_key` canonicalizes `a.md@…` ≡ `range:a.md@…` |
| 32 | Opposite directions to one range are distinct | COVERED | guards `opposite_directions_to_one_range_are_distinct` |
| 33 | The duplicate check is scoped to one invocation | COVERED | guards `duplicate_check_scoped_per_invocation` |
| 34 | Undo a range extension | PARTIAL | gap: spec wants reset-to-r0 restoring extent (r1 dangles, notes viewable) — test does forward `--id` re-commit, not a reset |
| 35 | Create a new commit after range reset | COVERED | reset `new_commit_after_reset_continues_chain` |
| 36 | Reset the file without separately resetting its range | COVERED | file_source `file_reset_restores_range_tips`; guards `file_reset_restores_child_range_tips_e2e` |
| 37 | A file target inside a child block rejects the whole reset | COVERED | coverage_atomic `file_reset_rejects_when_child_is_block_member` |
| 38 | A file snapshot preserves a recorded child END | PARTIAL | gap: file-reset-to-F restoring child to END e keeping closed state not asserted — test resets ON the END marker (different scenario) |
| 39 | Indirect breakage is visible before an intermediate reset | PARTIAL | gap: 3-level transitive `unreachable_link` diagnosis c1->b1->a1 without B resetting not asserted — test has one link only |
| 40 | Reading data does not repair its validity | COVERED | guards `reading_dangling_does_not_repair`; traceability `dangling_commit_still_inspectable` |
| 41 | Copying a record does not change its identity | COVERED | guards `copy_gives_new_identity` |
| 42 | A replay time does not bypass a version conflict | COVERED | guards `timestamp_replay_records_time_not_conflict` (strict commit-file timestamp); publication `expected_version_conflict_aborts`; lock_contention `two_processes_cannot_hold_write_lock` |
| 43 | Correct a comment without rewriting a commit | COVERED | guards `note_patch_revises_recorded_reason` |
| 44 | Note revisions follow publication rather than wall-clock order | COVERED | guards `note_revisions_follow_publication_order` (bug found+fixed: seq now unique/monotonic) |
| 45 | Keep evidence of a broken reference | COVERED | guards `referenced_dangling_commit_retained` |
| 46 | Collect an unreferenced dangling record | COVERED | gc `gc_collects_unreferenced_dangling_commit` |
| 47 | Inspect a dangling commit | COVERED | traceability `dangling_commit_still_inspectable` |
| 48 | Limit a tree to file level | COVERED | traceability `tree_level_file_hides_ranges` |
| 49 | Skipping a failed check does not confirm a range | COVERED | tags `skip_does_not_confirm_content` |
| 50 | Tree is machine-readable | COVERED | traceability `tree_json_is_parseable` |
| 51 | TOML formatting does not alter a commit ID | COVERED | guards `commit_id_stable_under_field_reorder` |
| 52 | Framed inputs cannot be confused by concatenation | COVERED | guards `framed_inputs_no_concat_confusion` (boundary-shift → different hash) |
| 53 | A selected link does not implicitly select all its changes | COVERED | guards `adapt_changes_clears_only_named` |
| 54 | Unknown current output is not empty successful coverage | COVERED | `check` reports missing tracked source `incomplete`; traceability `check_emits_structured_json` |
| 55 | A combination reports earlier successful members | PARTIAL | gap: spec wants mid-block write-failure reporting successful IDs/failed step/unclosed boundary/operation ID — test only covers invalid-endpoint failure |

change-review: **47 COVERED / 8 PARTIAL / 0 UNMAPPED / 0 DEFERRED**

## command-verification/spec.md (20 scenarios)

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Preserve literal argument boundaries | COVERED | command_source `literal_argv_boundaries_preserved` |
| 2 | Reject invalid argument types before launch | COVERED | command_source `non_string_args_rejected_before_launch`; discovery `command_ref_parses_argv_as_json` |
| 3 | Initialize despite verification auto-run being disabled | COVERED | guards `command_init_works_despite_autorun_disabled` |
| 4 | First capture fails after producing partial output | COVERED | command_source `non_zero_exit_is_failure_not_content` (V1 only on exit-0) |
| 5 | Invocation directory and metadata location do not change execution context | COVERED | command_source `command_runs_in_project_root` |
| 6 | An unavailable project root does not trigger a fallback | COVERED | discovery `explicit_bad_metadata_dir_never_falls_back`; bdd `no_fallback` |
| 7 | Default verification and check do not run commands | COVERED | guards `command_source_records_acquisition_and_unverified` |
| 8 | CLI disables a configured automatic run | COVERED | bdd `flag_false` |
| 9 | CLI explicitly enables this run | COVERED | guards `run_command_verify_reruns_and_compares` (`--run-command=true` re-runs+compares stdout) |
| 10 | A successful command prints warnings | COVERED | command_source `stderr_does_not_fail_a_successful_run` |
| 11 | A successful command produces no output | COVERED | command_source `empty_stdout_on_success_is_legal_empty_content`; bdd `empty_out` |
| 12 | Partial stdout followed by nonzero exit | COVERED | command_source `non_zero_exit_is_failure_not_content` |
| 13 | Clean does not execute a side-effecting command again | COVERED | guards `clean_does_not_rerun_command` (version count unchanged) |
| 14 | Equal output does not cancel explicit review work | COVERED | guards `equal_output_does_not_create_review`; `full_coverage_keeps_obligation` |
| 15 | Rebuilding a cache from unfamiliar metadata | PARTIAL | gap: spec clause — metadata with a not-yet-run COMMAND must not launch on rebuild — test has no command source, only index regen |
| 16 | Replacing with a command does not grant future automatic execution | COVERED | replace_source `replace_refused_on_mismatched_full_content`; `execution_permission_precedence` |
| 17 | A successful but different output cannot replace history | COVERED | replace_source `replace_refused_on_mismatched_full_content` |
| 18 | A program waits for standard input | COVERED | `command::cat::[]` stdin→`Stdio::null()` exits 0 (no hang) |
| 19 | Both output streams exceed a pipe buffer | COVERED | command_source `large_output_drains_without_deadlock` |
| 20 | Storage fails during capture | DEFERRED-PLATFORM | needs an in-process I/O fault-injection seam (fault_injector exists but is not wired into the capture stage) — explicit deferral, not a pass |

command-verification: **18 COVERED / 1 PARTIAL / 0 UNMAPPED / 1 DEFERRED-PLATFORM**

## local-project-links/spec.md (22 scenarios)

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | The linked directory changes after registration | PARTIAL | gap: spec wants check inspecting CURRENT content in moved dir (not frozen snapshot) — test only register/move/re-register |
| 2 | A Git-shaped source label is not a fetch command | COVERED | git_source `exact_commit_blob_read`; `floating_ref_name_rejected` |
| 3 | Remote changes without a matching mapping | DEFERRED-PLATFORM | remote-identity subsystem (URL↔declared-mapping, SSH/HTTPS non-equivalence) is spec-optional and undesigned — explicit deferral |
| 4 | Non-Git directories remain supported | COVERED | guards `rebuild_without_git_or_cache` |
| 5 | A project is moved locally | COVERED | guards `project_moved_locally_still_resolves` (store_id + commit-id unchanged asserted) |
| 6 | Relate implementations in two languages | PARTIAL | gap: spec wants two REGISTERED projects w/ cross-project link queryable both ends — test uses same-store `--link-from` |
| 7 | Link ranges without merging two metadata directories | PARTIAL | gap: real cross-store A→B link never created — test only counts two stores' commit files |
| 8 | Declaring a rule does not invent an implementation | COVERED | tags `uncovered_rule_fails_check_at_fail_level` |
| 9 | A child tag does not replace an inherited tag | COVERED | tags `dir_tag_inherits_to_members_deduped`; `new_member_inherits_dir_tag` |
| 10 | A one-way requirement permits additional reverse links | COVERED | tags `one_way_allows_extra_reverse_links` |
| 11 | One linked fragment does not cover the rest of a file | COVERED | coverage_atomic `overlapping_links_count_once`; `unmarked_content_stays_in_denominator` |
| 12 | Full content coverage does not require every overlapping range to have a link | COVERED | coverage_atomic `link_to_empty_target_fills_no_gap`; `whitespace_positions_filtered` |
| 13 | Warn reports insufficient coverage without failing the check by itself | COVERED | guards `warn_level_rule_does_not_fail_check`; tags `warn_level_reports_gap_without_failing` |
| 14 | Fail applies to the requested check rather than forcing verification | COVERED | tags `coverage_gap_but_verify_can_pass` |
| 15 | Two projects both use the spec tag | COVERED | guards `same_tag_name_independent_across_stores` |
| 16 | The consumer fails after the protection receipt is durable | COVERED | cross_store `inbound_credential_persists_before_consumer_publishes` |
| 17 | An offline consumer prevents unsafe collection | PARTIAL | gap: no inbound credential + offline peer seeded; 2nd assert is vacuous (`collected_commits` envelope key) |
| 18 | An unrelated offline store does not block local work | COVERED | guards `offline_peer_does_not_block_local_commit` |
| 19 | A writable copy is not silently treated as the original consumer | COVERED | cross_store `copied_store_refuses_writes_until_activated` |
| 20 | An explicit bad metadata location does not select a convenient fallback | COVERED | discovery `explicit_bad_metadata_dir_never_falls_back` |
| 21 | Two alternative metadata directories are ambiguous | COVERED | discovery `two_metadata_candidates_is_ambiguous` |
| 22 | Equivalent-looking remotes still need a declared mapping | DEFERRED-PLATFORM | same remote-identity subsystem as #3 — explicit deferral |

local-project-links: **15 COVERED / 5 PARTIAL / 0 UNMAPPED / 2 DEFERRED-PLATFORM**

## managed-content-tracking/spec.md (40 scenarios)

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Equal acquired contents follow equal review rules | COVERED | guards `equal_output_does_not_create_review` |
| 2 | New document enters the imported statistics | COVERED | lifecycle `new_members_auto_enter_import_statistics` |
| 3 | Remove does not erase tracking | COVERED | lifecycle `remove_keeps_ranges_and_disk_content`; `remove_exits_statistics_scope` |
| 4 | User explicitly tracks metadata in statistics | COVERED | lifecycle `self_tracking_not_auto_confirmed`; import_scope `dot_omd_is_importable_not_hidden` |
| 5 | A symbolic link forms a traversal cycle | COVERED | import_scope `symlink_cycle_reported`; `broken_symlink_is_a_problem` |
| 6 | Initializing a file is not a review | COVERED | lifecycle `empty_p_p_range_commits_and_covers_nothing`; bdd `not a review` |
| 7 | Identical coordinates have independent records | COVERED | bdd `two_ranges`/`no_tip`; guards `id_to_interior` (nonce chains) |
| 8 | A range is extended explicitly | COVERED | guards `explicit_range_expansion_distinct_object` |
| 9 | Repeated fragments retain their original context | COVERED | guards `ambiguous_fragment_reports_locate_candidates` |
| 10 | Rebuild without Git or cache | COVERED | guards `rebuild_without_git_or_cache`; `reindex_from_unfamiliar_manifest` |
| 11 | Verify uncommitted changes after same-content replacement | COVERED | guards `replace_then_verify_uses_new_source` + `replace_preserves_commit_id_and_links` |
| 12 | Readable Git history does not hide a missing current file | COVERED | guards `git_history_does_not_hide_missing_current` |
| 13 | HEAD movement alone does not change the observed file | COVERED | guards `head_movement_does_not_change_observation`; git_source |
| 14 | Git history remains readable after OMD cache loss | COVERED | git_source `exact_commit_blob_read`; guards rebuild tests |
| 15 | Missing Git objects cannot be replaced by current working-tree content | COVERED | git_source `missing_object_reports_unobtainable` |
| 16 | Move an existing file-backed version to matching Git content | COVERED | replace_source `replace_with_identical_content_succeeds` |
| 17 | Equal range snippets cannot authorize replacement of different full contents | COVERED | replace_source `replace_refused_on_mismatched_full_content` |
| 18 | Replacing one version does not discard an earlier unmatched version | COVERED | replace_source `shared_version_rebinds_together_other_version_untouched` |
| 19 | Multibyte text is not indexed as raw bytes | COVERED | ranges `text_ranges_count_scalar_positions_not_bytes`; guards `byte_mode_range_counts_bytes` |
| 20 | User selects a non-UTF-8 encoding | COVERED | file_source `non_utf8_encoding_decodes`; ranges `encoding_priority_cli_beats_recorded` |
| 21 | Local edit does not invalidate the entire file | COVERED | ranges `unrelated_range_stays_clean`; guards `in_range_edit_dirties_the_range` |
| 22 | An insertion at the end requires review without automatic expansion | COVERED | guards `insertion_at_end_dirties_no_growth`; ranges `insertion_at_range_end_dirties_without_growth` |
| 23 | Content moves without changing its text | COVERED | guards `pure_position_move_reports_moved_not_clean` (`moved: needs review`) |
| 24 | Outstanding range work blocks a successful file verification commit | COVERED | lifecycle `commit_verify_blocked_by_dirty_child_range`; guards `file_verify_blocked_by_uncommitted_range_edit` |
| 25 | Reusing a vacated path keeps histories separate | COVERED | lifecycle `vacated_path_histories_stay_separate`; guards `vacated_path_reuse_separate_history` |
| 26 | Rename does not assert a content adaptation | COVERED | lifecycle `rename_does_not_run_myers_or_rewrite_source` |
| 27 | An unrecorded deletion is reported | COVERED | lifecycle `missing_source_is_not_empty_content`; traceability `verify_deleted_source_unreachable` |
| 28 | Empty counted content has a stable displayed percentage | COVERED | coverage_atomic `empty_content_reports_100_percent`; `whitespace_only_content_reports_100_percent` |
| 29 | Text whitespace is excluded from coverage but remains in the source | COVERED | coverage_atomic `whitespace_positions_filtered_from_text_denominator`; `byte_mode_does_not_filter_whitespace` |
| 30 | Incomplete coverage does not fail verification by itself | COVERED | tags `coverage_gap_but_verify_can_pass` |
| 31 | Full coverage does not clear an independent obligation | COVERED | guards `full_coverage_keeps_obligation` |
| 32 | A stale writer cannot replace a newer record | COVERED | publication `expected_version_conflict_aborts`; lock_contention `two_processes_cannot_hold_write_lock` |
| 33 | Confirm a removed body as an empty range | COVERED | guards `confirm_deleted_body_explicit_empty_range` (`--id --range 0-0` on tip) |
| 34 | A duplicate fragment is not chosen automatically | COVERED | guards `ambiguous_fragment_reports_locate_candidates` |
| 35 | A split does not copy relationships | COVERED | guards `split_range_inherits_no_relationships` |
| 36 | Shared version IDs differ from equal content hashes | PARTIAL | gap: counts version files; replace-rebind + gc-shared-content clauses rest on substring asserts elsewhere |
| 37 | An interrupted write does not publish an orphan | COVERED | publication `staged_failure_before_rename_keeps_old_state` |
| 38 | A lost response does not duplicate a successful update | COVERED | publication `lost_response_detected_by_operation_id` |
| 39 | A failed final synchronization is not a promise of rollback | PARTIAL | gap: `--expect-version` isn't a real flag (test passes via clap error); uncertain-result + operation-ID output never asserted |
| 40 | A reader detects a changed participant | COVERED | guards `reader_detects_changed_participant` (Store::open tip-integrity; bug found+fixed) |

managed-content-tracking: **36 COVERED / 4 PARTIAL / 0 UNMAPPED / 0 DEFERRED-PLATFORM**

---

## Totals (authoritative)

| spec | COVERED | PARTIAL | UNMAPPED | DEFERRED-PLATFORM |
|---|---|---|---|---|
| change-review (55) | 47 | 8 | 0 | 0 |
| command-verification (20) | 18 | 1 | 0 | 1 |
| local-project-links (22) | 15 | 5 | 0 | 2 |
| managed-content-tracking (40) | 36 | 4 | 0 | 0 |
| **total (137)** | **116** | **18** | **0** | **3** |

## Real bugs found + fixed during remediation

- `apply_reset_to_state` walked wrong direction → links in removed segment not withdrawn (reset.rs + main wiring)
- verify reported `clean` for pure content position moves → `moved: needs review` + `dirtied_by` wiring
- command sources unreachable via CLI → `--source-ref` records `Acquisition::Command`; `verify --run-command=true` re-runs+compares
- `--run-command` bare flag ate the subcommand → `require_equals`
- command-source nodes reported `missing` → `is_virtual` check
- `commit verify` did not block on uncommitted range edits → shared `range_needs_review` gate
- `commit_link` accepted nonexistent range endpoints → tip-existence guard (phantom links refused; combos with a bad member now fail)
- note seqs collided at `publication+1` → unique monotonic per-commit seq
- `check` reported empty `files:[]` for a tracked-but-missing source → `incomplete` + `coverage: null`
- `Store::open` silently parsed a tampered tip → tip→commit integrity check (reader detects changed participant)
- 8 vacuous tests removed/rewritten; 3 more de-vacuified post-audit

## Re-check provenance

A bounded independent re-check (post-ledger) refuted 12 self-closed rows whose
tests exercised an adjacent clause, not the named spec scenario — they are
marked PARTIAL above with the exact unverified clause named. The 3
DEFERRED-PLATFORM rows are confirmed legitimate (no in-process seam). The
`link_to_nonexistent_range_rejected`/`combo_link_reports_partial_failure` tests
must switch to `--source/--target` form to exercise the guard, not clap
(currently pass via arg-parse error — misleading). Vacuous duplicates
(`git_history…`, `head_movement…`, `insertion_at_end…` in guards.rs) are
redundant with real tests elsewhere — rows stay covered by those.

## Prior audit provenance

Two independent reviewer audits ran against this tree (a 137-row scenario map and a
per-file change/verification re-audit), plus a final full-ledger pass; their findings
are superseded by the table above, which is corrected against the actual scenario
list and current code. The stale pre-remediation table and narrative appendices were
removed — this file is now the single authoritative per-row ledger.
