//! Test document compilation and diagnostics handling.

use std::fmt::Debug;

use ecow::eco_vec;
use ecow::EcoVec;
use thiserror::Error;
use typst::diag::Severity;
use typst::diag::SourceDiagnostic;
use typst::diag::Warned;
use typst::layout::PagedDocument;
use typst::syntax::Source;
use typst::World;
use tytanic_utils::fmt::Term;

/// How to handle warnings during compilation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Warnings {
    /// Ignore all warnings.
    Ignore,

    /// Emit all warnings.
    #[default]
    Emit,

    /// Promote all warnings to errors.
    Promote,
}

/// An error which may occur during compilation. This struct only exists to
/// implement [`Error`][trait@std::error::Error].
#[derive(Debug, Clone, Error)]
#[error("compilation failed with {} {}", .0.len(), Term::simple("error").with(.0.len()))]
pub struct Error(pub EcoVec<SourceDiagnostic>);

/// Compiles a test using the given test world.
pub fn compile(
    _source: Source,
    world: &dyn World,
    warnings: Warnings,
) -> Warned<Result<PagedDocument, Error>> {
    let Warned {
        output,
        warnings: mut emitted,
    } = typst::compile(world);

    // if let Some((old, new)) = adapter.accessed_old.into_inner() {
    //     emitted.push(SourceDiagnostic {
    //         severity: Severity::Warning,
    //         span: Span::detached(),
    //         message: eco_format!("Accessed {old} in tests for package {new}"),
    //         trace: eco_vec![],
    //         hints: eco_vec![eco_format!(
    //             "Did you forget to update the package import in your template?"
    //         )],
    //     });
    // }

    match warnings {
        Warnings::Ignore => Warned {
            output: output.map_err(Error),
            warnings: eco_vec![],
        },
        Warnings::Emit => Warned {
            output: output.map_err(Error),
            warnings: emitted,
        },
        Warnings::Promote => {
            emitted = emitted
                .into_iter()
                .map(|mut warning| {
                    warning.severity = Severity::Error;
                    warning.with_hint("this warning was promoted to an error")
                })
                .collect();

            match output {
                Ok(doc) if emitted.is_empty() => Warned {
                    output: Ok(doc),
                    warnings: eco_vec![],
                },
                Ok(_) => Warned {
                    output: Err(Error(emitted)),
                    warnings: eco_vec![],
                },
                Err(errors) => {
                    emitted.extend(errors);
                    Warned {
                        output: Err(Error(emitted)),
                        warnings: eco_vec![],
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_builder::file::VirtualFileProvider;
    use crate::world_builder::library::LibraryProvider;
    use crate::world_builder::test_utils;

    const TEST_PASS: &str = "Hello World";
    const TEST_WARN: &str = "#set text(font: \"foo\"); Hello World";
    const TEST_FAIL: &str = "#set text(font: \"foo\"); #panic()";

    #[test]
    fn test_compile_pass_ignore_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::new();
        let source = Source::detached(TEST_PASS);
        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        let Warned { output, warnings } = compile(source, &world, Warnings::Ignore);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_pass_emit_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::new();
        let source = Source::detached(TEST_PASS);
        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        let Warned { output, warnings } = compile(source, &world, Warnings::Emit);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_pass_promote_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::new();
        let source = Source::detached(TEST_PASS);
        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        let Warned { output, warnings } = compile(source, &world, Warnings::Promote);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_warn_ignore_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::new();
        let source = Source::detached(TEST_WARN);
        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        let Warned { output, warnings } = compile(source, &world, Warnings::Ignore);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_warn_emit_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::new();
        let source = Source::detached(TEST_WARN);
        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        let Warned { output, warnings } = compile(source, &world, Warnings::Emit);
        assert!(output.is_ok());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_compile_warn_promote_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::new();
        let source = Source::detached(TEST_WARN);
        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        let Warned { output, warnings } = compile(source, &world, Warnings::Promote);
        assert_eq!(output.unwrap_err().0.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_fail_ignore_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::new();
        let source = Source::detached(TEST_FAIL);
        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        let Warned { output, warnings } = compile(source, &world, Warnings::Ignore);
        assert_eq!(output.unwrap_err().0.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_fail_emit_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::new();
        let source = Source::detached(TEST_FAIL);
        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        let Warned { output, warnings } = compile(source, &world, Warnings::Emit);
        assert_eq!(output.unwrap_err().0.len(), 1);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_compile_fail_promote_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::new();
        let source = Source::detached(TEST_FAIL);
        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        let Warned { output, warnings } = compile(source, &world, Warnings::Promote);
        assert_eq!(output.unwrap_err().0.len(), 2);
        assert!(warnings.is_empty());
    }
}
