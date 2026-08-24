---
lesson: hermetic-test-fixtures
---

# Hermetic Test Fixtures

Two scanner tests read a fixture header through a hardcoded relative path into a sibling repository; they passed on the original monorepo machine and failed everywhere else, including CI.

## Symptom

```text
---- scanner::cpp::tests::real_skill_fragments_header_extracts_base_class stdout ----
thread panicked at src\scanner\cpp.rs:780:53:
read SkillFragment.h: Os { code: 3, kind: NotFound, message: "系统找不到指定的路径。" }
```

74 of 76 unit tests passed; the two failures shared one cause: `../LyraStarterGame/Plugins/.../SkillFragment.h` did not exist outside the original repository layout.

## Root Cause

The tests were written against the author's machine layout, not against the crate. A relative `../` path silently couples a test to whatever happens to live next to the repository clone — the worst kind of implicit environment dependency, because it passes locally and fails for every other contributor and for CI.

## Fix

- Copy the real fixture into the crate at `alexandria/tests/fixtures/SkillFragment.h` (the file was 19 lines — small enough to vendor byte-identically, no trimming needed).
- Reference it with `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/SkillFragment.h")` so the path resolves from the crate root regardless of checkout location or working directory.

## Guard

- Any test that reads outside the crate directory is a bug, even when it passes locally. Grep for `"../"` in test code during review.
- Prefer `CARGO_MANIFEST_DIR`-anchored paths over cwd-relative paths in tests; cwd differs between `cargo test`, IDE runners, and CI.
- When vendoring a fixture, take the real content over a synthetic mock — the fixture's value is that it exercises real-world input.

## Evidence

- The vendored fixture: `alexandria/tests/fixtures/SkillFragment.h`.
- The repointed tests: `alexandria/src/scanner/cpp.rs` (`real_skill_fragments_header_extracts_base_class`, `diagnostic_real_skill_fragments_parse_tree`).
