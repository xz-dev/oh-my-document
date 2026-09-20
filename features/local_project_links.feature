@local-project-links
Feature: local project links and source references
  Aliases bind explicit local dirs; source references parse by fixed prefix;
  links connect range commits, never file nodes; cross-store endpoints fix
  target store + version.

  Background:
    Given a clean OMD project

  Rule: Source references parse by fixed prefix first

  Scenario: proj path keeps everything after the alias colon
      Given a source ref "proj:A:docs/deep::file.md"
      Then the path is "docs/deep::file.md"

  Scenario: byte marker selects raw-byte mode
      Given a source ref "proj:A:byte::bin.dat"
      Then the ref uses byte offsets

  Scenario: command argv parses as a JSON array
      Given a source ref "command::tool::[\"x\", \"y\"]"
      Then the executable is "tool" with 2 args

  Rule: Discovery never silently falls back

  Scenario: An explicit bad metadata location does not select a convenient fallback
      Given an explicit meta path that does not exist
      When I resolve the metadata dir
      Then the error reports the explicit location, no fallback write

  Scenario: Two alternative metadata directories are ambiguous
      Given a root with two direct children each holding a manifest
      When I resolve the metadata dir
      Then ambiguity is reported

  Rule: Links connect range commits only

  Scenario: A file node cannot be a link endpoint
      Given a file "a.md" containing "x"
      And I run "omd init a.md"
      When I run "omd commit link a.md --link-from file:a.md --link-to file:b.md"
      Then the link is refused

    @wip
    Scenario: Adapt selects by link id
      Given a pending adaptation
      When I run "omd commit adapt --id <link-id> --no-reason"
      Then that pending entry is cleared
