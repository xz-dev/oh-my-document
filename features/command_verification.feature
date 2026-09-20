@command-verification
Feature: command source verification
  command sources are explicit argv run at the project root; only successful
  full stdout is tracked; permission is opt-in and never inherited.

  Background:
    Given a clean OMD project

  Rule: argv is literal, never re-joined or templated

  Scenario: argv is not re-joined into a shell string
      Given a command source "command::tool::[\"a\", \"b c\"]"
      Then the executable is "tool" and args are exactly two

  Scenario: Non-string argv elements are rejected before launch
      Given a command source "command::tool::[123]"
      Then parsing fails before the program runs

  Rule: Only successful full stdout becomes a tracked version

  Scenario: Empty stdout with exit 0 is a valid version
      Given a command source that exits 0 with empty stdout
      Then the source version is empty content

  Scenario: Partial stdout never becomes a new baseline
      Given a command source that exits nonzero after writing partial stdout
      Then the partial output is not a successful version

  Rule: Permission is opt-in per invocation

  Scenario: Built-in default denies execution
      Given no explicit flag and no config
      Then command execution is not permitted

  Scenario: Config true is overridden by explicit false
      Given config auto_run is true
      When explicit flag false is passed
      Then execution is denied for this call
