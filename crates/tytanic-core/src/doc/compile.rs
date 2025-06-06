//! Test document compilation and diagnostics handling.

use std::fmt::Debug;
use std::sync::{LazyLock, OnceLock};

use ecow::{eco_format, eco_vec, EcoVec};
use thiserror::Error;
use typst::diag::{FileResult, Severity, SourceDiagnostic, Warned};
use typst::foundations::{Bytes, Datetime};
use typst::layout::PagedDocument;
use typst::syntax::package::PackageSpec;
use typst::syntax::{FileId, Source, Span};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use tytanic_utils::fmt::Term;

use crate::library::augmented_default_library;
use crate::world_builder::TestWorld;

static AUGMENTED_LIBRARY: LazyLock<LazyHash<Library>> =
    LazyLock::new(|| LazyHash::new(augmented_default_library()));

/// A wrapper type around World implementations for compiling tests.
///
/// This type is used to check self package accesses.
#[derive(Clone)]
struct TestWorldAdapter<'w> {
    base: &'w dyn World,
    package: Option<PackageSpec>,
    accessed_old: OnceLock<(PackageSpec, PackageSpec)>,
}

impl TestWorldAdapter<'_> {
    fn check_access(&self, id: FileId) {
        let Some(this) = self.package.as_ref() else {
            return;
        };

        let Some(package) = id.package() else {
            return;
        };

        if package.namespace == this.namespace
            && package.name == this.name
            && package.version < this.version
        {
            _ = self.accessed_old.set((package.clone(), this.clone()));
        }
    }
}

impl World for TestWorldAdapter<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.base.library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.base.book()
    }

    fn main(&self) -> FileId {
        self.base.main()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.check_access(id);
        self.base.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.check_access(id);
        self.base.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.base.font(index)
    }

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        self.base.today(offset)
    }
}

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
pub fn compile(world: TestWorld, warnings: Warnings) -> Warned<Result<PagedDocument, Error>> {
    let adapter = TestWorldAdapter {
        base: &world,
        package: todo!(),
        accessed_old: todo!(),
    };

    let Warned {
        output,
        warnings: mut emitted,
    } = typst::compile(&adapter);

    if let Some((old, new)) = adapter.accessed_old.into_inner() {
        emitted.push(SourceDiagnostic {
            severity: Severity::Warning,
            span: Span::detached(),
            message: eco_format!("Accessed {old} in tests for package {new}"),
            trace: eco_vec![],
            hints: eco_vec![eco_format!(
                "Did you forget to update the package import in your template?"
            )],
        });
    }

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
    use crate::world_builder::library::DefaultLibraryProvider;
    use crate::world_builder::virtual_world;

    const TEST_PASS: &str = "Hello World";
    const TEST_WARN: &str = "#set text(font: \"foo\"); Hello World";
    const TEST_FAIL: &str = "#set text(font: \"foo\"); #panic()";

    #[test]
    fn test_compile_pass_ignore_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = DefaultLibraryProvider::new();
        let source = Source::detached(TEST_PASS);
        let world = virtual_world(source, &mut files, &library);

        let Warned { output, warnings } = compile(world, Warnings::Ignore);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_pass_emit_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = DefaultLibraryProvider::new();
        let source = Source::detached(TEST_PASS);
        let world = virtual_world(source, &mut files, &library);

        let Warned { output, warnings } = compile(world, Warnings::Emit);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_pass_promote_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = DefaultLibraryProvider::new();
        let source = Source::detached(TEST_PASS);
        let world = virtual_world(source, &mut files, &library);

        let Warned { output, warnings } = compile(world, Warnings::Promote);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_warn_ignore_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = DefaultLibraryProvider::new();
        let source = Source::detached(TEST_WARN);
        let world = virtual_world(source, &mut files, &library);

        let Warned { output, warnings } = compile(world, Warnings::Ignore);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_warn_emit_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = DefaultLibraryProvider::new();
        let source = Source::detached(TEST_WARN);
        let world = virtual_world(source, &mut files, &library);

        let Warned { output, warnings } = compile(world, Warnings::Emit);
        assert!(output.is_ok());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_compile_warn_promote_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = DefaultLibraryProvider::new();
        let source = Source::detached(TEST_WARN);
        let world = virtual_world(source, &mut files, &library);

        let Warned { output, warnings } = compile(world, Warnings::Promote);
        assert_eq!(output.unwrap_err().0.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_fail_ignore_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = DefaultLibraryProvider::new();
        let source = Source::detached(TEST_FAIL);
        let world = virtual_world(source, &mut files, &library);

        let Warned { output, warnings } = compile(world, Warnings::Ignore);
        assert_eq!(output.unwrap_err().0.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_fail_emit_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = DefaultLibraryProvider::new();
        let source = Source::detached(TEST_FAIL);
        let world = virtual_world(source, &mut files, &library);

        let Warned { output, warnings } = compile(world, Warnings::Emit);
        assert_eq!(output.unwrap_err().0.len(), 1);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_compile_fail_promote_warnings() {
        let mut files = VirtualFileProvider::new();
        let library = DefaultLibraryProvider::new();
        let source = Source::detached(TEST_FAIL);
        let world = virtual_world(source, &mut files, &library);

        let Warned { output, warnings } = compile(world, Warnings::Promote);
        assert_eq!(output.unwrap_err().0.len(), 2);
        assert!(warnings.is_empty());
    }
}
