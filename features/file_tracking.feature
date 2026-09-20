@tracking
Feature: file tracking lifecycle
  OMD records immutable commits for managed files so that history is
  auditable and reset-capable without Git.

  Background:
    Given a clean OMD project

  Rule: A file must be explicitly initialized before it is tracked

    Scenario: initializing a file records an init commit
      Given a file "docs/a.md" containing "hello"
      When I run "omd init docs/a.md"
      Then the command succeeds
      And a commit of kind "init" exists for "file:docs/a.md"
      And "file:docs/a.md" has a non-empty tip

    Scenario: an uninitialized file reports no history
      Given a file "docs/b.md" containing "world"
      When I query the tip of "file:docs/b.md"
      Then it has no tip
