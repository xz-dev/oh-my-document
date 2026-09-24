# oh-my-document

**English** | [简体中文](README.zh-CN.md)

**Connect the system, not just the files.**

A requirement says “retry at most 3 times.” Later, someone changes it to 5. Where is the retry code—and did anyone update it too? Which implementation matches the logic in the design diagram and the test that checks the retry limit?

OMD (`omd`) links exact ranges in requirements, designs, diagrams, code, and tests. People and agents can follow those links to understand a system, find what a change affects, and record why it was reviewed or handled a certain way. Links can connect documents to documents, code to code, or implementations across languages and projects—not just documentation to code, and not just to remind someone to update the docs.

Files stay where they belong; OMD maintains their relationships. Object identity stays stable when ranges move, and content changes reveal which ranges and links need review. People and agents use the same CLI and checks.

## Design: an agent as the interpreter between intent and implementation

OMD is designed **first for agents to use**. It aims to bridge the gap between human algorithm, business, and architecture design and agent-written code. Design decisions need a life beyond a conversation, and generated code needs a traceable connection to the intent behind it.

**People own architecture and process; AI implements the code.** People define goals, constraints, and key decisions. Agents follow the relationships among requirements, designs, implementations, and tests to work out details and review changes. OMD preserves specific ranges, version evidence, and reasons so that people can return to the design and check how it was realized.

The reverse path matters too: people can use agents to **understand an existing project from the ground up, in a structured way**. Start with its purpose and components, then explore workflows, algorithms, and code fragments, establishing and checking relationships as you go. This gives a person a way to ask “why this design, where is it implemented, and what verifies it?” and remain able to change the system.

This direction builds on [Treating LLMs as interpreters: managing context](https://xzos.net/blog/llm-as-interpreter-context-management/): treat context and workflows as the program an agent executes, and use explicit design, relationships, and verification to make that role more reliable. **This is an engineering goal, not a guarantee of determinism or semantic equivalence.** People retain responsibility for design and acceptance. OMD checks relationships and change records; tests and reviews assess implementation. A link or passing check alone does not prove business correctness.

## Install

Install [Rust and Cargo](https://rustup.rs/), then install directly from the repository:

```bash
cargo install --git https://github.com/xz-dev/oh-my-document.git --locked
```

Or clone and install from a checkout:

```bash
git clone https://github.com/xz-dev/oh-my-document.git
cd oh-my-document
cargo install --path . --locked
```

Run `omd --help`. Current development and verification focus on Linux; Windows and macOS are not claimed by the present test evidence.

## Run a complete local workflow

### From “at most 3 retries” to “at most 5”

Understand the task before deciding whether to run it manually. This is an illustrative collaboration, not a transcript of an agent run. The corresponding CLI steps were checked in an isolated directory; see the [hands-on tutorial](docs/tutorials/retry.md).

**1. A person defines the design; an agent creates an inspectable relationship.**

Start with two fragments:

```text
spec.md:  Retry at most 3 times.
retry.py: MAX_RETRIES = 3
```

You might ask an agent:

> Read the requirement and implementation. Link “at most 3 retries” to the code responsible for that limit, and explain why. Show me the selected fragments first; do not confirm entire files.

Inspect the actual fragments and the reason for `requirement range → implementation range`: this code expresses the required limit. OMD records range identity, content versions, and a separate link. Placing files in one directory—or running init—does not establish that relationship.

**2. When the requirement changes, follow the relationship to the code.**

Change the requirement to 5 retries but leave the code alone. The actual result for this example is:

```text
verify: exit 1, ok: false
dirty: range:<requirement range ID> — in-range edit …
```

This is an excerpt for readability; IDs differ between runs. It tells you the tracked requirement changed. Following the link leads to the range in `retry.py` that still says `MAX_RETRIES = 3`. You need not remember its location or ask the agent to guess the project's structure again.

**3. A person decides the change; an agent implements and records its response.**

> Accept the new limit of 5. Inspect the linked implementation, change the constant, run the relevant check, and record which requirement change this implementation revision handles, on which link, and why.

The minimal example checks only that the constant is 5. A real project must also test its retry loop, error paths, and side effects. The agent uses fresh observation evidence to continue the existing ranges and explicitly records the link, change version, and adaptation reason. Review the implementation and evidence, not merely the agent's “done.”

**4. What does a passing check mean?**

After this response is recorded, the example's `verify` returns `ok: true`, with no dirty, locate, or outstanding-review items. Original range identity remains, and new versions and reasons are traceable. However, the current CLI may also pass after recording only the new requirement, before the implementation is updated. **A green check alone does not establish that code caught up.** Here, consistency was assessed by following the link, reading the code, running a check, and reviewing it—not by an OMD semantic proof.

Want to try it? The [hands-on tutorial](docs/tutorials/retry.md) explains the purpose, commands, results, and decisions step by step. JSON and ID handling are real costs of the current CLI and are not omitted there; they should not be prerequisites for understanding the tool. Maintainer assertion scripts live separately under [regression examples](examples/regression/README.md).

## Manage files and directories

`omd import README.md` includes one file in statistics; `omd import docs` includes a directory and discovers new members recursively. Import is independent of `init`: neither import nor initialization confirms a content range. `remove` withdraws statistics without deleting the source or its content history. Each write to an existing store still needs fresh `--expected` evidence.

File lists and tag coverage use the latest effective import selection. Missing files or incomplete traversal fail `check`; a successful check without configured rules does not imply full coverage. Use the current build: older binaries reject stores containing separate import and content objects at the same path.

Start with the repository's [OMD skill](skills/omd/SKILL.md); see [actual self-management and limits](docs/omd-self-management.md).

## Source fields

Source kind and coordinates are separate:

- **File is the default.** `omd init docs/spec.md` observes that project-relative path. Use `--source-project` and `--source-path` when recovery content belongs to another registered project.
- **Command is explicit and literal.** Use `--source-type command --executable <program> --args-json '<JSON string array>'`. OMD passes argv without an implicit shell, tracks complete stdout only after exit 0, and does not execute commands during `list`, `log`, `tree`, or cache rebuild. Later `verify`/`check` execution requires explicit permission.
- **Git history is exact and local.** Use `--source-type git --source-project <alias> --git-commit <full object id> --git-path <path in that commit>`. Floating refs, automatic clone/fetch, and using HEAD as current file content are rejected.
- `--mode text|byte` selects coordinate units; it does not select a provider. File paths remain literal, including `@`, `#`, `%`, spaces, colons where the platform permits them, and Unicode.

## Configuration-first initialization

A genuinely new project may contain only `.omd/omd.toml` before its first explicit `init`. This supports an encoding default such as:

```toml
format = "omd.encoding/1"
default_encoding = "windows-1252"
```

The first initialization preserves those configuration bytes and uses the configured text view. Existing, damaged, externally selected, or already mapped metadata is not treated as a bootstrap target. Reading configuration does not execute a source command or grant normal write authority.

## Local mappings, relocation, and copies

`omd project register <alias> <project-root> <metadata-dir>` stores machine-local placement in `projects.toml`; shared history keeps logical project/store identity and project-relative paths. Registration and relocation mutations require a fresh `verify` observation through `--expected`, like other writes.

Moving one authoritative store can keep its store ID after explicit remapping. Copying metadata to another writable directory is different: the raw copy is read-only until explicit `omd activate`, which assigns a new store ID and completes required peer protection. Existing external references to the original store do not silently redirect to the copy. OMD does not synchronize or merge the two writable histories.

## Operational boundaries

- Authoritative structured text, immutable records, and required content live under `.omd/` by default; the query index under `OMD_CACHE_PATH` is rebuildable.
- `OMD_CONFIG_PATH` and `OMD_CACHE_PATH` take precedence over platform/XDG fallbacks. `--root`, `--meta`, `--project`, and `--store` select explicit context; invalid explicit locations do not fall back silently.
- Writes require fresh caller evidence. Stale publication, source, registration, mapping, or peer evidence fails rather than rebasing or retrying against newer state.
- Cross-store publication protects referenced versions before publishing the consumer. Late I/O failure can truthfully leave already published members and an open block; JSON reports those members, failed step, boundary, and operation ID instead of claiming rollback.
- Unsupported old stores and removed compound source/coordinate interfaces are rejected without migration or rewriting.

## Read more

- [Requirements and design history](docs/requirements.md)
- [Sources and coordinates](docs/source-model.md)
- [Storage and paths](docs/storage.md)
- [Implementation handoff](docs/handoff.md)
- [Spec-to-task evidence](spec-traceability.md)

Current Rust core and CLI exist, but overall change acceptance still depends on independent review. An initial repository-local OMD skill is available; it is not globally installed and not every workflow has been exercised. The optional Lean workflow has not been delivered or validated. OMD does not claim semantic equivalence, document sufficiency, or all-platform support.

## Contributing

Run repository checks with:

```bash
cargo fmt --check
cargo build --locked --bin omd
cargo test --locked
```

## License

No license has been chosen. No open-source license is currently granted.
