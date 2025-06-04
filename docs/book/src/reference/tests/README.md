# Tests
There are three types of tests:
- Unit tests, which are similar to unit or integration tests in other languages and are mostly used to test the API of a package and visual regressions through comparison with reference documents.
  Unit tests are standalone files in a `tests` directory inside the project root and have additional features available inside typst using a custom standard library.
- Template tests, these are automatically created from a template packge's template directory and may not access the augmented standard library.
  Note that there are also unit tests which can access the template directory assets.
  Instead, they receive access to the template assets.
- Doc tests, example code in documentation comments which are compiled but not compared.

<div class="warning">

Tytanic can currently not collect doc tests.

These will be added in the future, see [#34].

</div>

Any unit test may use [annotations](./annotations.md) for configuration.

Read the [guide], if you want to see some examples on how to write and run various tests.

## Sections
- [Unit tests](./unit.md) explains the structure of unit tests.
- [Template tests](./template.md) the usage of template tests.
- [Test library](./lib.md) lists the declarations of the custom standard library.
- [Annotations](./annotations.md) lists the syntax for annotations and which are available.

[guide]: ../../guides/tests.md
[#34]: https://github.com/typst-community/tytanic/issues/34
