# [unreleased](https://github.com/tingerrr/tytanic/releases/tag/)
## Highlights

## Changes
- Added template tests.
- Added `unit()` test set.
- Added `template()` test set.
- Added absolute paths to unit tests in `--json` outout
- Added `template_test` to `status --json`
- Removed `is_template` form `status --json`
- `update` no longer updates all matching test, but only those which fail
- Added `--force` flag to `update` to foce updating all matching tests
- Added `util completion` sub command for generating completions
- Added `util manpage` sub command for generating man pages
- Unhid non-default switches on short help
- Added `--include-persistent-references` to `util clean` sub command
- Added filter options to `util clean` sub command
- Re-licensed under `MIT OR Apache-2.0`

## Fixes
- Don't panic when trying to update non-persistent tests
- Don't report old version of typst in `util about`

---

# [v0.2.2](https://github.com/tingerrr/tytanic/releases/tag/v0.2.2)
## Changes
- Update MSRV to `1.84`
- Update dependencies
- Update Typst to `0.13.1`

---

# [v0.2.1](https://github.com/tingerrr/tytanic/releases/tag/v0.2.1)
## Fixes
- Do not ignore critical error paths

---

# [v0.2.0](https://github.com/tingerrr/tytanic/releases/tag/v0.2.0)
## Highlights
This release bumps Typst to `0.13.0` and brings plenty of improvements like annotations and config options for various export and comparison settings.
Among other things this release focuses on improved reproducibility by no longer reading system fonts by default.

> [!important]
> This release was yanked, see v0.2.1.

## Changes
- Updated to Typst `0.13.0`
- Added `--[no-]use-embedded-fonts`
- Renamed `--ignore-system-fonts` to `--no-use-system-fonts`
- Added `--use-system-fonts`
- **BREAKING CHANGE**: `--no-use-system-fonts` is now the default
- Renamed `--creation-timestamp` to `--timestamp`
- **BREAKING CHANGE**: `--timestamp` now defaults to `0`
- Wraps the help message according to the terminal width
- Renamed regression tests to unit tests
- **BREAKING CHANGE**: broken manifests will cause a hard error again
- Added `tool.tytanic.tests` manifest config option for configuring the unit test root
- Added `tool.tytanic.default` manifest config section for configuring defaults, contains options for `ppi`, `dir`, `max-deviations` and `max-delta`.
- Added `ppi`, `dir`, `max-deviations` and `max-delta` annotations.
- Raw patterns in test set expressions no longer parse `,`, `(` or `)`

## Fixes
- Don't remove references of persistent tests on `tt util clean`
- Register `compile-only()` not `compile_only()`

---

# [v0.1.3](https://github.com/tingerrr/tytanic/releases/tag/v0.1.3)
## Fixes
- Don't delete persistent references on `tt run`

---

# [v0.1.2](https://github.com/tingerrr/tytanic/releases/tag/v0.1.2)
## Highlights
This release adds a workflow for releasing Docker containers to `ghrc.io/tingerrr/tytanic` for each subsequent.

> [!important]
> This release was yanked, see v0.1.3.

## Changes
- Added `--compare` (as inverse of `--no-compare`)
- Added `--fail-fast` (as inverse of `--no-fail-fast`)
- Renamed `--no-implicit-skip` to `--no-skip`,  added `--skip`
- Renamed `--no-optimize-references` to `--no-optimize-refs`, added `--optimize-refs`
- Renamed `--no-export` to `--no-export-ephemeral`, added `--export-ephemeral`
- Removed `--promote-warnings`
- Added `--warnings`
- Added `--type`
- Added `--persistent`
- Added docker release workflow and dockerfile
- Deprecated `tt add` in favor of `tt new`
- Deprecated `tt remove` in favor of `tt delete`
- Made migration of nested tests optional

## Fixes
- Don't ignore single explicit test arguments
- Don't create ddefault test with incorrect reference
- Fix detection of git repository
- Don't create persistent references for non-persistent tests
- Don't panic with empty annotations
- Don't panic if `tests` doesn't exist
- Don't discard warnings
- Don't panic if manifest is invalid
- Ensure commands which receive explicitly passed tests fail when a test is missing

---

# [v0.1.1](https://github.com/tingerrr/tytanic/releases/tag/v0.1.1)
## Changes
- Added `tt util vcs ignore` to regenerate ignore files
- Show if template is detected in `tt status`
- Changed location of ignore files to be in the test directory itself

## Fixes
- Write correct header for mercurial ignore files
- Removed a dead asset path from the `flake.nix`
- Don't panic when optimizing reference documents
- Don't panic when running `tt add` with faulty template
- Respect `--no-fail-fast` for test failures
- Don't comparison early if `--no-fail-fast` is not used

---

# [v0.1.0](https://github.com/tingerrr/tytanic/releases/tag/v0.1.0)
## Highlights
This is the initial release of Tytanic, it now hosts an mdBook using GitHub pages and contains many new features:
- test sets
- more granular comparison
- ephemeral and compile-only tests
- an augmented library
- a built-in Typst compiler

## Changes
- Added a test set DSL for filtering tests
- Added compile-only tests which only compiled
- Added ephemeral tests which create references on the go
- Added support for skipping tests usiong in-source annotations
- Added test templates using `tests/template.typ`
- Added an augmented standard library with special helpers for regression test
- Added `--font-path` for adding additional font search paths
- Added `--ignore-system-fonts` for disabling system fonts
- Added `--creation-timestamp` for configuring the `datetime.now()` timestmap
- Added `--max-deviations` `--min-delta` options for configuring comparison thresholds
- Added `--json` to print the output of some commands as JSON to stdout
- Added an mdBook containing various guides and reference documents
- Removed `tt config`
- Removed `tt init`
- Removed `tt uninit`
- Removed `--format`
- Changed the default test structure
- Added `tt util migrate` to migrate to the new directory structure

## Fixed
- Don't panic on `tt add` with non-default test template
- Ensure VCS ignore files are not removed when running tests
- Compress test references when running `tt update`
