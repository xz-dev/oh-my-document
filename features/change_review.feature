@change-review
Feature: change review and coverage
  Dirty ranges and obligations are tracked per commit; confirm/clean commits
  clear obligations; coverage reports only unexpired confirmed ranges.

  Background:
    Given a clean OMD project

  Rule: Coverage counts only unexpired confirmed ranges

      @wip
  Scenario: A dirty position stays dirty even under a confirmed range
      Given a file "docs/d.md" with a confirmed range and a dirty range overlapping one position
      When I check coverage
      Then that position counts as dirty
      And coverage is not 100%

      @wip
  Scenario: Coverage is empty for a file with no confirmed ranges
      Given a file "docs/nc.md" containing "text"
      When I check coverage
      Then coverage is reported for tracked positions only

  Rule: Confirm commits clear obligations; clean commits truncate propagation

      @wip
  Scenario: Confirm without reason resolves the dirty obligation
      Given a dirty range on "docs/o.md"
      When I run "omd commit confirm docs/o.md --no--reason"
      Then the obligation is cleared

      @wip
  Scenario: Clean blocks upstream dirty propagation
      Given a dirty upstream range linked to a downstream range
      When I run "omd commit clean <downstream> --stop"
      Then the downstream is not marked dirty

  Rule: reset only lands on placeholder marker commits

      @wip
  Scenario: Reset targeting an atomic block collapses to the predecessor
      Given a closed atomic block
      When I reset to its END commit
      Then the actual landing is the block's predecessor
      And a warning records the requested-vs-actual difference

  Rule: Unclean commits stack obligations

      @wip
  Scenario: Multiple unclean commits retain distinct obligation ids
      Given an open obligation
      When I run "omd commit unclean docs/u.md"
      Then the new obligation has a distinct id and both stack
