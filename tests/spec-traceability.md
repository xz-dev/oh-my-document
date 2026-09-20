# Spec → Test traceability

Task 13.1 requires every scenario in the four capability specs to map to a
real automated test or an explicit platform verification record. This table is
the living index. Entries are added as their owning tasks are implemented;
"planned" means the scenario exists but its test does not yet.

Each row names the spec requirement, the owning task number, and the test
target (unit `src/…` or integration `tests/…`). Statuses: `planned`,
`implemented`, `verified`.

| Spec file | Requirement | Task | Test target | Status |
|---|---|---|---|---|
| managed-content-tracking | Content sources share tracking behavior | 4.x/9.x/10.x | tests/content_tracking.rs | planned |
| managed-content-tracking | Import statistics are separate from file tracking | 5.4 | tests/import_scope.rs | planned |
| managed-content-tracking | Import has explicit scope adjustments | 5.4 | tests/import_scope.rs | planned |
| managed-content-tracking | Files are initialized explicitly | 4.1 |  tests/file_source.rs | implemented |
| managed-content-tracking | Range identity is independent of coordinates | 4.2 |  tests/ranges.rs | implemented |
| managed-content-tracking | Full source versions survive cache loss | 4.x | tests/content_tracking.rs | planned |
| managed-content-tracking | Git references identify historical contents | 10.1 | tests/git_sources.rs | planned |
| managed-content-tracking | Git source history reuses repository objects | 10.1 | tests/git_sources.rs | planned |
| managed-content-tracking | Source replacement preserves the complete recorded version | 10.2/10.3 | tests/replace.rs | planned |
| managed-content-tracking | Text and byte sources retain distinct coordinate units | 4.2 |  tests/ranges.rs | implemented |
| managed-content-tracking | Verification identifies affected ranges | 4.3 |  tests/ranges.rs | implemented |
| managed-content-tracking | File hash commits do not clear range obligations | 4.5 | tests/content_tracking.rs | planned |
| managed-content-tracking | Paths can change without merging identities | 5.3 | tests/path_lifecycle.rs | planned |
| managed-content-tracking | Tombstones and missing sources remain distinguishable | 5.3 | tests/path_lifecycle.rs | planned |
| managed-content-tracking | Coverage uses only current confirmations | 7.1 |  tests/coverage_atomic.rs | implemented |
| managed-content-tracking | Authority and write preconditions remain independent of cache | 3.x |  tests/publication.rs | implemented |
| managed-content-tracking | Ambiguous or deleted ranges require explicit coordinate commits | 4.4 | tests/range_coordinates.rs | planned |
| managed-content-tracking | Replacement targets a complete source version binding | 10.2/10.3 | tests/replace.rs | planned |
| managed-content-tracking | Single updates publish through one authoritative state selection | 3.x |  tests/publication.rs | implemented |
| change-review | Links connect range tracking objects | 6.1 | tests/links.rs | planned |
| change-review | Link instances have distinct persistent identities | 6.1 |  tests/domain_semantics.rs | implemented |
| change-review | Links require an explicit user operation | 6.1 | tests/links.rs | planned |
| change-review | Adaptation identifies the link and selected changes | 6.2 |  tests/domain_semantics.rs | implemented |
| change-review | Clean is a source-side branch stop | 6.2 | tests/links.rs | planned |
| change-review | Unclean preserves separate obligations | 6.3 |  tests/domain_semantics.rs | implemented |
| change-review | Cycles do not cause infinite traversal | 6.3 | tests/links.rs | planned |
| change-review | Atomic blocks group one range commit chain | 8.x |  tests/coverage_atomic.rs | implemented |
| change-review | Open atomic blocks advance current state but fail closure | 8.x |  tests/coverage_atomic.rs | implemented |
| change-review | Ordinary atomic members cannot be reset independently | 8.x |  tests/coverage_atomic.rs | implemented |
| change-review | Directional link options create an atomic combination | 8.4 | tests/atomic.rs | planned |
| change-review | Range reset restores the selected commit point | 8.2 | tests/reset.rs | planned |
| change-review | File reset restores the ranges at the target commit | 8.2 | tests/reset.rs | planned |
| change-review | Broken required references are diagnosed transitively | 6.4 |  tests/domain_semantics.rs | implemented |
| change-review | Commit identity preserves original inputs | 2.2 |  src/records/tests.rs | implemented |
| change-review | Explicit timestamps support manual replay | 2.2 | tests/records.rs | planned |
| change-review | Notes are separate append-only records | 12.1 | tests/notes.rs | planned |
| change-review | Garbage collection is explicit and protects referenced records | 12.2 | tests/gc.rs | planned |
| change-review | History and tree queries do not mutate validity | 12.3 | tests/queries.rs | planned |
| change-review | JSON and explicit skipping preserve meaning | 12.4 | tests/json_output.rs | planned |
| change-review | Canonical records preserve all immutable hash inputs | 2.2 |  src/records/tests.rs | implemented |
| change-review | CLI selections distinguish versions links and changes | 6.x/12.x | tests/links.rs | planned |
| change-review | JSON output and exit codes expose actual outcomes | 12.4 | tests/json_output.rs | planned |
| command-verification | A command object uses a fixed executable and literal args | 9.1 | tests/commands.rs | planned |
| command-verification | Explicit initialization captures the first complete output | 9.2 | tests/commands.rs | planned |
| command-verification | Command execution uses the owning project root | 9.1 | tests/commands.rs | planned |
| command-verification | Verification and coverage checks share execution precedence | 9.2 | tests/commands.rs | planned |
| command-verification | Only complete stdout from a normal successful exit is content | 9.3 | tests/commands.rs | planned |
| command-verification | Failed capture cannot advance the source version | 9.3 | tests/commands.rs | planned |
| command-verification | Captured output is reused for review rather than rerun | 9.3 | tests/commands.rs | planned |
| command-verification | Loading or rebuilding metadata is not execution consent | 9.2 | tests/commands.rs | planned |
| command-verification | Explicit command replacement authorizes one complete capture | 10.3 | tests/replace.rs | planned |
| command-verification | The executor preserves complete output and fixed context | 9.1 | tests/commands.rs | planned |
| local-project-links | Project aliases bind explicit local directories | 11.1 | tests/cross_store.rs | planned |
| local-project-links | Optional remote identity checks do not introduce force | 11.1 | tests/cross_store.rs | planned |
| local-project-links | Project registration identity does not rewrite commits | 11.1 | tests/cross_store.rs | planned |
| local-project-links | Cross-project relationships preserve the range boundary | 11.x | tests/cross_store.rs | planned |
| local-project-links | Tags and named checks express relationship requirements | 7.2 | tests/coverage.rs | planned |
| local-project-links | Tag relationship coverage measures content not object counts | 7.1 |  tests/coverage_atomic.rs | implemented |
| local-project-links | Rule severity and skipping are explicit | 7.2 | tests/coverage.rs | planned |
| local-project-links | Tags have project-local names | 7.2 | tests/coverage.rs | planned |
| local-project-links | Cross-store publication protects referenced versions first | 11.2/11.3 | tests/cross_store.rs | planned |
| local-project-links | Store copies and moves preserve explicit authority | 11.4 | tests/cross_store.rs | planned |
| local-project-links | Local discovery and source parsing are deterministic | 5.1/5.2 | tests/discovery.rs | planned |
