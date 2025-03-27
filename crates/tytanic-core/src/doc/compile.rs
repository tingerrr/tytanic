//! Test document compilation and diagnostics handling.

use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::{LazyLock, OnceLock};

use ecow::{eco_format, eco_vec, EcoVec};
use thiserror::Error;
use typst::diag::{FileResult, Severity, SourceDiagnostic, Warned};
use typst::foundations::{Bytes, Datetime};
use typst::layout::PagedDocument;
use typst::syntax::package::PackageSpec;
use typst::syntax::{FileId, Source, Span, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use tytanic_utils::fmt::Term;

use crate::library::augmented_default_library;

static AUGMENTED_LIBRARY: LazyLock<LazyHash<Library>> =
    LazyLock::new(|| LazyHash::new(augmented_default_library()));

/// A wrapper type around World implementations for compiling tests.
///
/// This type is exposed only within [`compile`].
#[derive(Clone)]
pub struct TestWorldAdapter<'w> {
    base: &'w dyn World,
    source: Source,
    root_prefix: Option<PathBuf>,
    augment: bool,
    package: Option<PackageSpec>,
    accessed_old: OnceLock<(PackageSpec, PackageSpec)>,
}

impl TestWorldAdapter<'_> {
    /// Whether the standard library should be augmented.
    ///
    /// This can be used to allow unit tests to test error paths and inspect
    /// panics. This can be omitted if the base world implementation already has
    /// an augmented standard library, see [`augmented_library`][lib].
    ///
    /// [lib]: crate::library::augmented_library
    pub fn augment_standard_library(&mut self, value: bool) -> &mut Self {
        self.augment = value;
        self
    }

    /// Add a root prefix to each [`FileId`].
    ///
    /// This can be used to allow template tests to access the correct files
    /// when using absolute paths. A template test located in `$root/template`
    /// would have absolute file id like `/refs.bib` resolve to
    /// `/template/refs.bib` after which the base [`World`] implementation takes
    /// with resolving the path over.
    pub fn root_prefix(&mut self, value: Option<PathBuf>) -> &mut Self {
        self.root_prefix = value;
        self
    }

    /// Set the given package spec to be re-routed to the current project root.
    ///
    /// This can be used to allow template tests to import unreleased versions
    /// of the currently tested package. Any [`FileId`] with this package spec
    /// will stripped of it, accessing the project files instead.
    pub fn reroute_package(&mut self, value: Option<PackageSpec>) -> &mut Self {
        self.package = value;
        self
    }
}

impl TestWorldAdapter<'_> {
    fn transform_id(&self, id: FileId) -> FileId {
        let Some(this) = self.package.as_ref() else {
            return id;
        };

        match id.package() {
            Some(package) if package == this => FileId::new(None, id.vpath().clone()),
            Some(package) => {
                if package.namespace == this.namespace
                    && package.name == this.name
                    && package.version < this.version
                {
                    _ = self.accessed_old.set((package.clone(), this.clone()));
                }

                id
            }
            None => match self.root_prefix.as_ref() {
                Some(prefix) => FileId::new(
                    None,
                    VirtualPath::new(prefix.join(id.vpath().as_rootless_path())),
                ),
                None => id,
            },
        }
    }
}

impl World for TestWorldAdapter<'_> {
    fn library(&self) -> &LazyHash<Library> {
        if self.augment {
            &AUGMENTED_LIBRARY
        } else {
            self.base.library()
        }
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.base.book()
    }

    fn main(&self) -> FileId {
        match self.root_prefix.as_ref() {
            Some(prefix) => {
                let id = self.source.id();
                let vpath = id.vpath().as_rootless_path();

                FileId::new(
                    id.package().cloned(),
                    VirtualPath::new(vpath.strip_prefix(prefix).unwrap_or(vpath)),
                )
            }
            None => self.source.id(),
        }
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        let id = self.transform_id(id);

        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            self.base.source(id)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let id = self.transform_id(id);

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

/// Compiles a source with the given global world.
///
/// This function compiles a test source by wrapping the provided [`World`]
/// implementation in a short lived wrapper which exposes `source` as the
/// compilation entrypoint. The `f` argument can be used to configure the
/// behavior of this wrapper type.
pub fn compile<'w, F>(
    source: Source,
    world: &'w dyn World,
    warnings: Warnings,
    f: F,
) -> Warned<Result<PagedDocument, Error>>
where
    F: for<'a> FnOnce(&'a mut TestWorldAdapter<'w>) -> &'a mut TestWorldAdapter<'w>,
{
    let mut test_world = TestWorldAdapter {
        base: world,
        source,
        root_prefix: None,
        augment: false,
        package: None,
        accessed_old: OnceLock::new(),
    };

    let Warned {
        output,
        warnings: mut emitted,
    } = typst::compile(f(&mut test_world));

    if let Some((old, new)) = test_world.accessed_old.into_inner() {
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
    use crate::_dev::VirtualWorld;

    const TEST_PASS: &str = "Hello World";
    const TEST_WARN: &str = "#set text(font: \"foo\"); Hello World";
    const TEST_FAIL: &str = "#set text(font: \"foo\"); #panic()";

    #[test]
    fn test_compile_pass_ignore_warnings() {
        let world = VirtualWorld::default();
        let source = Source::detached(TEST_PASS);

        let Warned { output, warnings } = compile(source, &world, Warnings::Ignore, |w| w);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_pass_emit_warnings() {
        let world = VirtualWorld::default();
        let source = Source::detached(TEST_PASS);

        let Warned { output, warnings } = compile(source, &world, Warnings::Emit, |w| w);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_pass_promote_warnings() {
        let world = VirtualWorld::default();
        let source = Source::detached(TEST_PASS);

        let Warned { output, warnings } = compile(source, &world, Warnings::Promote, |w| w);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_warn_ignore_warnings() {
        let world = VirtualWorld::default();
        let source = Source::detached(TEST_WARN);

        let Warned { output, warnings } = compile(source, &world, Warnings::Ignore, |w| w);
        assert!(output.is_ok());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_warn_emit_warnings() {
        let world = VirtualWorld::default();
        let source = Source::detached(TEST_WARN);

        let Warned { output, warnings } = compile(source, &world, Warnings::Emit, |w| w);
        assert!(output.is_ok());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_compile_warn_promote_warnings() {
        let world = VirtualWorld::default();
        let source = Source::detached(TEST_WARN);

        let Warned { output, warnings } = compile(source, &world, Warnings::Promote, |w| w);
        assert_eq!(output.unwrap_err().0.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_fail_ignore_warnings() {
        let world = VirtualWorld::default();
        let source = Source::detached(TEST_FAIL);

        let Warned { output, warnings } = compile(source, &world, Warnings::Ignore, |w| w);
        assert_eq!(output.unwrap_err().0.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compile_fail_emit_warnings() {
        let world = VirtualWorld::default();
        let source = Source::detached(TEST_FAIL);

        let Warned { output, warnings } = compile(source, &world, Warnings::Emit, |w| w);
        assert_eq!(output.unwrap_err().0.len(), 1);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_compile_fail_promote_warnings() {
        let world = VirtualWorld::default();
        let source = Source::detached(TEST_FAIL);

        let Warned { output, warnings } = compile(source, &world, Warnings::Promote, |w| w);
        assert_eq!(output.unwrap_err().0.len(), 2);
        assert!(warnings.is_empty());
    }
}
