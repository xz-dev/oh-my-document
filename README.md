# oh-my-document

**English** | [简体中文](README.zh-CN.md)

**When code changes, don't leave the docs behind.**

[Why agents?](#why-build-for-agents) · [Explore a project](#understand-a-project-step-by-step) · [Use cases](#what-can-you-link) · [Install](#install) · [Try it](#try-it-link-a-requirement-to-code) · [Docs](#read-more) · [Feedback](https://github.com/xz-dev/oh-my-document/issues)

A requirement says “retry at most 3 times.” Later, someone changes it to 5. Where is the retry code—and did anyone update it too?

**OMD (`omd`) maps code, documentation, and UML diagrams to help people explore projects with an agent, understand agent-written code, and guide changes.** It links specific passages in requirements and designs to implementation ranges, tracks content changes, and records why those changes were handled a certain way.

The current entry point is a Rust CLI that both people and agents can use. Markdown, diagram source, and code stay where they are; tracking records live in `.omd/`.

## Why build for agents?

After an agent writes some code, you still need to know: Which requirement led to this implementation? Does it follow the design? What else needs checking after this change? If those answers exist only in a chat, the next person has to ask again—or read the code from scratch.

OMD is designed to keep those connections in the project. You can define requirements, flows, and component relationships, then let an agent work down to functions and implementation details. When the direction needs to change, the linked design and code give you concrete places to intervene.

**Predictability here means having grounds to judge a change, not predicting the agent's next line of code:**

- **Understand the reasoning.** Which requirement or design does this code correspond to, and why was it changed?
- **Know what to review.** Follow recorded links to check what changed and what still needs attention.
- **Guide the next edit.** Change a requirement or design, identify the implementations an agent should review, and keep a record of the outcome and reasoning.

People or agents must create these links explicitly; OMD does not infer every dependency. The goal is to give you something to check beyond an agent's claim that the work is done.

OMD is designed as **a second layer of assurance for agent-written code**: the agent makes edits, the tool performs repeatable content and rule checks, and people judge the result and adjust direction. It complements tests and code review. A link—or a passing check—does not prove that an implementation meets its requirements or is free of defects.

OMD does not take over coding or require a particular agent. People and agents follow the same rules; automation gets no relaxed checks.

## Understand a project step by step

When joining an unfamiliar project, you can ask an agent to explain the existing code through documentation and UML diagrams, then use OMD to link those explanations to actual source ranges. Start with the question in front of you and go deeper as needed; you do not have to read the entire repository first.

1. **Start with the big picture.** Ask the agent to describe the project's purpose, main components, and relationships through documentation and UML diagrams.
2. **Follow a question deeper.** Pick a flow you want to understand. Have the agent explain its modules, states, and branches, linking the explanation and diagram source to the relevant code.
3. **Check against the implementation.** Follow those mappings back to the code, ask about unclear parts, and correct or expand the explanation. The links stay in the project for later exploration and change checks.

This works in both directions: when building, work from requirements and UML toward implementation; when learning, start with existing code, build explanations and diagrams, then explore the details. People or agents write the explanations and diagrams. OMD maintains explicit links so you can check their basis—it does not certify an explanation as fact.

## What can you link?

| What you're working with | How to connect it |
| --- | --- |
| A requirement and its implementation | Link the relevant passage to the code so you have specific content to review after a change |
| A state diagram and business logic | Link states and branches in the diagram source to the functions that handle them |
| Two implementations of the same algorithm | Link corresponding ranges and record how each change was handled |

Choose the ranges you care about; you do not have to treat a whole document or file as one unit. File tracking does not depend on a Git repository, and you can keep your editor, document formats, and diagramming tools.

## Install

Install [Rust and Cargo](https://rustup.rs/), then build from source:

```bash
git clone https://github.com/xz-dev/oh-my-document.git
cd oh-my-document
cargo install --path . --locked
```

Run `omd --help` to explore the commands. Development and verification currently focus on Linux.

## Try it: link a requirement to code

This example uses two small files and Bash. No Git repository required.

### 1. Write a requirement and its implementation

```bash
mkdir omd-demo
cd omd-demo

printf 'Retry at most 3 times.\n' > spec.md
printf 'MAX_RETRIES = 3\n' > retry.py
```

| `spec.md` | `retry.py` |
| --- | --- |
| Retry at most **3** times. | `MAX_RETRIES = 3` |

### 2. Tell OMD that these ranges are related

```bash
omd init spec.md
omd init retry.py

omd commit commit spec.md --range 0-22 \
  --reason "Require at most 3 retries"

omd commit commit retry.py --range 0-15 \
  --link-from "spec.md@text:0-22" \
  --reason "Implement the retry limit with MAX_RETRIES"
```

Ranges use **zero-based character positions, with an inclusive start and an exclusive end**, not line numbers. `0-22` selects `Retry at most 3 times.`; `0-15` selects `MAX_RETRIES = 3`. Neither includes the trailing newline.

`init` registers a file, `--range` selects a passage, `--link-from` creates a link, and `--reason` records the reasoning. OMD stores its `commit` records in `.omd/`; these are separate from Git commits.

### 3. Change the requirement and check

```bash
printf 'Retry at most 5 times.\n' > spec.md
omd verify
```

The check returns JSON with `data.ok` set to `false` and `range:spec.md@text:0-22` listed in `data.dirty`. That requirement has changed and needs review; `retry.py` still says `3`.

You can now decide whether the implementation needs updating and record the outcome and reasoning in OMD. It will not change the code to `5` for you.

**Known issue in this example:** after creating the link above, the current version also reports `version record missing` for the linked range in `retry.py`, even before either file is edited. The changed requirement is detected, but this linked verification flow is not yet working end to end. The result above reflects that limitation, not a successful validation.

## Fit it into your workflow

Start with one requirement and implementation that often change together, then add links as needed. When working with an agent, you can make linking, checking changes, and recording the reasoning part of the task. People can review the result with the same commands.

- **What changed?** Run `omd verify` to find tracked ranges that need review.
- **What's missing a link?** Configure tags and link rules, then run `omd check` to inspect coverage. Rules can warn or fail the check.
- **What happened before?** Use `omd list` to find current commit IDs, then `omd log <commit-id>` to read a chain's history.
- **Want to script it?** Use `--json` for structured output without replacing your existing development workflow.

Records live in the project's `.omd/` directory by default. Use `--meta <directory>` to choose another location. See `omd --help` and `omd commit --help` for more options.

## Read more

The README introduces the tool and walks through an example. For design background and detailed contracts, see these documents, currently in Chinese:

- [Requirements and design goals](docs/requirements.md) — why OMD tracks content ranges, links, and reasoning.
- [Sources and coordinates](docs/source-model.md) — contracts for files, command output, character ranges, and byte ranges.
- [Storage and paths](docs/storage.md) — what metadata and indexes store.
- [Spec-to-implementation traceability](spec-traceability.md) — implementation evidence and remaining gaps.

These documents include design-stage contracts; consult the current CLI help for command syntax. Remote URL identity mapping and the optional Lean product skill are not yet available.

## Feedback and contributing

Try linking a small, real piece of documentation to its implementation, then check what happens when it changes. If the result is unexpected, open an [issue](https://github.com/xz-dev/oh-my-document/issues) with the commands, relevant file excerpts, and the behavior you expected.

To work on the code, run the tests from the repository:

```bash
cargo test --all-targets
```

## License

A license has not been chosen. No open-source license is currently granted.
