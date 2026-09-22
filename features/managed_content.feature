@managed-content
Feature: managed content tracking
  OMD tracks file and command sources through explicit commits, ranges, and
  source versions — recoverable without Git or cache.

  Background:
    Given a clean OMD project

  Rule: Files are initialized explicitly

    Scenario: Initializing a file is not a review
      Given a file "docs/spec.md" containing "spec text"
      When I run "omd init docs/spec.md"
      Then the command succeeds
      And "file:docs/spec.md" has a non-empty tip
      And no range coverage is claimed for "file:docs/spec.md"

  Rule: Range identity is independent of coordinates

    Scenario: Identical coordinates have independent records
      Given a file "docs/r.md" containing "content"
      And I run "omd init docs/r.md"
      When I run "omd commit commit docs/r.md --range 0 3"
      And I run "omd commit commit docs/r.md --range 0 3"
      Then two distinct range chains exist for "docs/r.md"

  Rule: Full source versions survive cache loss

    @wip
    Scenario: Rebuild without Git or cache
      Given a file "docs/v.md" containing "persist me"
      And I run "omd init docs/v.md"
      When the query cache is removed
      Then the recorded source content is still recoverable

  Rule: Text and byte sources retain distinct coordinate units

    @wip
    Scenario: Multibyte text is not indexed as raw bytes
      Given a file "docs/mb.md" containing "汉字abc"
      Then text coordinates count Unicode scalar positions
      And byte coordinates count raw byte offsets

  Rule: Verification identifies affected ranges without semantic claims

    @wip
    Scenario: An insertion at the end requires review without automatic expansion
      Given a confirmed range covering only "校验密码。"
      When the content becomes "校验密码。记录日志。然后返回结果。"
      Then the original range becomes dirty
      And the range is not auto-expanded

    @wip
    Scenario: Local edit does not invalidate the entire file
      Given two disjoint confirmed ranges
      When an edit touches only the first
      Then the first range reports dirty
      And the second stays confirmed

  Rule: File hash commits do not clear range obligations

    @wip
    Scenario: Outstanding range work blocks a successful file verification commit
      Given a file with a still-dirty range
      When I run "omd commit verify docs/x.md"
      Then the verification does not pass

  Rule: Paths can change without merging identities

    @wip
    Scenario: Reusing a vacated path keeps histories separate
      Given a tracked file with prior history
      When a different file is renamed onto that path
      Then the two histories are not merged
