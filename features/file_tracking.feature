@tracking
Feature: explicit file and range tracking
  OMD records immutable commits for managed files and ranges so that
  history is auditable without Git.

  Background:
    Given a clean OMD project

  Rule: A file is registered by an explicit init commit

    Scenario: initializing a file records an init record
      Given a file "docs/a.md" containing "hello"
      When I run "omd init docs/a.md"
      Then the command succeeds
      And "file:docs/a.md" has a non-empty tip

    Scenario: init is not a review
      Given a file "docs/b.md" containing "world"
      When I run "omd init docs/b.md"
      Then the command succeeds
      And no range coverage is claimed for "file:docs/b.md"

  Rule: Identical coordinates have independent records

    Scenario: same range twice without --id makes two objects
      Given a file "docs/c.md" containing "content"
      And I run "omd init docs/c.md"
      When I run "omd commit commit docs/c.md --range 0-3"
      And I run "omd commit commit docs/c.md --range 0-3"
      Then two distinct range chains exist for "docs/c.md"
