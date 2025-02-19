# Template Test
This section refers to the virtual template test created for a package with a template directory, for unit tests with a template annotation refer to [unit tests].

## Import Translation
The import for the package itself is automatically resolved to the local project directory.
This way template test can run on unpublished versions without installing the package locally.

## Template Root
Assets and files inside the template test are resolved relative to the template directory, not the project root.
This ensures that a template test will run just like a fresh invocaiton of `typst compile` after `typst init`.

[unit tests]: ./unit.md
