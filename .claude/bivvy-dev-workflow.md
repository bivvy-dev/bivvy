# Bivvy Development Workflow

> An atomic, test-driven development workflow for building Bivvy.

<steps>
  <step number="1" id="specification">
    <title>Specification (Test-Driven)</title>
    <goal>Define the contract your code must fulfill before writing implementation.</goal>
    <instructions>
      <instruction>
        Define the Behavior: Create test files first. Tests are the specification.
        <conditional>
          <condition>Test files already exist</condition>
          <action>Update tests to match the new/changed behavior</action>
        </conditional>
      </instruction>
      <instruction>
        <command>cargo test</command>
        <expected>All new or updated tests should FAIL</expected>
        <rationale>Failing tests prove they're actually testing new logic, not passing by accident.</rationale>
      </instruction>
    </instructions>
    <gate>
      <condition>New tests fail for the right reasons</condition>
      <on_fail>Refine your tests until they correctly specify the behavior</on_fail>
    </gate>
  </step>

  <step number="2" id="implementation">
    <title>Implementation</title>
    <goal>Write the minimum code to make tests pass.</goal>
    <prerequisite ref="specification">Failing tests from specification step</prerequisite>
    <instructions>
      <instruction>
        Write the Code: Implement only what's needed to pass the tests. Resist adding "nice to have" features.
      </instruction>
      <instruction>
        Keep it Simple: If you're writing more code than tests require, stop and reconsider.
      </instruction>
    </instructions>
    <gate>
      <condition>All tests pass</condition>
      <on_fail>Continue implementing or fix bugs until green</on_fail>
    </gate>
  </step>

  <step number="3" id="documentation">
    <title>Documentation</title>
    <goal>Document while context is fresh.</goal>
    <prerequisite ref="implementation">Tests passing</prerequisite>
    <instructions>
      <instruction>
        Rustdoc Comments: Add `///` doc comments to all public items (structs, functions, modules).
      </instruction>
      <instruction>
        User-Facing Docs: Update `docs/` if adding commands, config options, or user-visible features.
      </instruction>
      <instruction>
        Inline Comments: Add comments only where the "why" isn't obvious from the code.
      </instruction>
    </instructions>
    <anti-patterns>
      <anti-pattern>Documenting "what" the code does (the code should show that)</anti-pattern>
      <anti-pattern>Leaving documentation for later (it won't happen)</anti-pattern>
      <anti-pattern>Over-commenting obvious code</anti-pattern>
    </anti-patterns>
  </step>

  <step number="4" id="linting">
    <title>Static Quality Gate (Linting)</title>
    <goal>Catch issues before they become problems.</goal>
    <prerequisite ref="documentation">Implementation and documentation complete</prerequisite>
    <instructions>
      <instruction>
        <command>cargo fmt -- --check</command>
        <expected>No formatting differences</expected>
        <fix>cargo fmt</fix>
      </instruction>
      <instruction>
        <command>cargo clippy --all-targets --all-features -- -D warnings</command>
        <expected>No warnings or errors</expected>
        <rationale>Warnings today become bugs tomorrow. Zero tolerance. Flags match CI.</rationale>
      </instruction>
    </instructions>
    <gate>
      <condition>Both commands pass with zero output</condition>
      <on_fail goto="implementation">Fix all issues. Return to implementation if code changes are needed.</on_fail>
    </gate>
  </step>

  <step number="5" id="testing">
    <title>Verification Gate (Full Test Suite)</title>
    <goal>Ensure nothing is broken.</goal>
    <prerequisite ref="linting">Linting passes</prerequisite>
    <critical>
      ALWAYS run `cargo test --all-features` without filters. NEVER use:
      - cargo test some_name
      - cargo test module::
      - cargo test --lib
      The ENTIRE test suite must pass with all features. "X filtered out" means you ran it wrong.
      The --all-features flag matches CI - skipping it locally may cause CI failures.
    </critical>
    <instructions>
      <instruction>
        <command>cargo test --all-features</command>
        <expected>All tests pass (new and existing), 0 filtered out</expected>
      </instruction>
      <instruction>
        <command>cargo llvm-cov --all-features --fail-under-lines 90</command>
        <expected>Coverage at or above 90%</expected>
        <conditional>
          <condition>cargo-llvm-cov not installed</condition>
          <action>Skip coverage check locally; CI will enforce</action>
        </conditional>
      </instruction>
    </instructions>
    <gate>
      <condition>All tests pass, coverage threshold met</condition>
      <on_fail goto="implementation">Do not proceed with failing tests.</on_fail>
    </gate>
  </step>

  <step number="6" id="build">
    <title>Integration Gate (Build Verification)</title>
    <goal>Confirm production readiness.</goal>
    <prerequisite ref="testing">All tests passing</prerequisite>
    <instructions>
      <instruction>
        <command>cargo build --all-targets --all-features</command>
        <expected>Build succeeds with no warnings</expected>
        <rationale>RUSTFLAGS="-D warnings" in CI means any warning fails the build. Flags match CI.</rationale>
      </instruction>
      <instruction>
        <command>cargo build --release</command>
        <expected>Release build succeeds</expected>
        <rationale>Catches release-only issues (optimizations, LTO problems).</rationale>
      </instruction>
    </instructions>
    <gate>
      <condition>Both builds succeed silently</condition>
      <on_fail goto="linting">Treat warnings as errors. Return to linting or implementation until clean.</on_fail>
    </gate>
  </step>

  <step number="7" id="commit">
    <title>The Atomic Commit</title>
    <goal>Create a single, complete unit of work.</goal>
    <prerequisite ref="build">All gates passed (lint, test, build)</prerequisite>
    <instructions>
      <instruction>
        Stage Selectively: Add only the files related to this change.
        <command>git add [specific files]</command>
        <anti-pattern>git add -A (may include unrelated changes)</anti-pattern>
      </instruction>
      <instruction>
        Write the Message: Explain WHY, not just WHAT.
        <format>
          <line>type(scope): short description</line>
          <line></line>
          <line>Longer explanation of why this change was made,</line>
          <line>what problem it solves, and any important context.</line>
        </format>
        <types>feat, fix, docs, test, refactor, chore, ci</types>
      </instruction>
      <instruction>
        Verify Atomicity: This commit should be independently checkable, buildable, testable, and documented.
      </instruction>
    </instructions>
    <gate>
      <condition>Commit contains code + tests + docs for one logical change</condition>
      <on_fail>Split into smaller commits or add missing pieces</on_fail>
    </gate>
  </step>

  <step number="8" id="post-commit">
    <title>Post-Commit Verification</title>
    <goal>Confirm the commit didn't break anything.</goal>
    <prerequisite ref="commit">Commit created</prerequisite>
    <instructions>
      <instruction>
        <command>cargo test</command>
        <expected>Still passing after commit</expected>
      </instruction>
      <instruction>
        <command>cargo build</command>
        <expected>Build succeeds with no warnings</expected>
      </instruction>
      <instruction>
        <command>git log -1 --stat</command>
        <rationale>Review what was actually committed</rationale>
      </instruction>
    </instructions>
  </step>

  <step number="9" id="transition">
    <title>Transition</title>
    <goal>Clean context switch to next task.</goal>
    <instructions>
      <instruction>
        Clean Slate: Clear your mental context. The previous task is done.
      </instruction>
      <instruction goto="specification">
        Next Task: Pick the next item from your backlog and return to specification.
      </instruction>
    </instructions>
  </step>
</steps>

<verification-commands>
  <description>Quick reference for all verification commands in order (matches CI):</description>
  <command-sequence>
    <command>cargo fmt -- --check</command>
    <command>cargo clippy --all-targets --all-features -- -D warnings</command>
    <command note="NO FILTERS - run entire suite with all features">cargo test --all-features</command>
    <command>cargo build --all-targets --all-features</command>
    <command>cargo build --release</command>
    <command>cargo llvm-cov --all-features --fail-under-lines 90</command>
  </command-sequence>
  <critical>
    `cargo test --all-features` must be run WITHOUT name filters. Never filter tests.
    Output must show "0 filtered out" - any other number is wrong.
    All flags (--all-targets, --all-features) match CI exactly.
  </critical>
</verification-commands>

<principles>
  <principle name="Atomic">Each commit is complete and independent</principle>
  <principle name="Test-First">Tests define behavior before implementation</principle>
  <principle name="Zero-Warning">Warnings are errors; fix them immediately</principle>
  <principle name="Document-As-You-Go">Docs are part of the deliverable, not an afterthought</principle>
  <principle name="Small-Steps">Prefer many small commits over few large ones</principle>
</principles>
