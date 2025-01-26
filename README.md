# Tytanic
`tytanic` is a test runner for [Typst] projects.
It helps you worry less about regressions and speeds up your development.

## Features
Out of the box `tytanic` supports the following features:
- compile and compare tests
- manage regression tests of various types
- manage and update reference documents when tests change
- filter tests effectively for concise test runs

[![An asciicast showing tytanic running the full cetz test suite.][demo-thumb]][demo]

## Stability & Typst version
`tytanic` currently targets Typst `0.12.0` only, the CLI will remain stable across patch versions, but may change across.

## Documentation
To see how to get started with `tytanic`, check out the [book].

## Contribution
[CONTRIBUTING.md][contrib] contains some guidelines for contributing.

## Changelog
The changelog can be found [here][changelog].

[contrib]: ./CONTRIBUTING.md
[changelog]: ./CHANGELOG.md

[workaround]: https://tingerrr.github.io/tytanic/guides/watching.html
[Typst]: https://typst.app
[book]: https://tingerrr.github.io/tytanic/index.html

[demo-thumb]: https://asciinema.org/a/rW9HGUBbtBnmkSddgbKb7hRlI.svg
[demo]: https://asciinema.org/a/rW9HGUBbtBnmkSddgbKb7hRlI
