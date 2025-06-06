//! Unified diagnostic printing for XML export and CLI tools.

use std::path::Path;

use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::diagnostic::Label;
use codespan_reporting::files::Error;
use codespan_reporting::files::Files;
use codespan_reporting::term::emit;
use codespan_reporting::term::Config;
use ecow::eco_format;
use termcolor::WriteColor;
use typst::diag::Severity;
use typst::diag::SourceDiagnostic;
use typst::syntax::FileId;
use typst::syntax::Source;
use typst::syntax::Span;
use typst::World;
use typst::WorldExt;

/// Writes Typst diagnostics to a writer with colors if possible.
///
/// The given root path is used to resolve paths and should point to the root
/// used by the world.
///
/// # Panics
/// Panics if the diagnostics have spans pointing to files not found by the
/// given world.
pub fn write_diagnostics(
    w: &mut dyn WriteColor,
    diagnostic_config: &Config,
    world: &dyn World,
    root: &Path,
    warnings: &[SourceDiagnostic],
    errors: &[SourceDiagnostic],
) -> Result<(), Error> {
    fn resolve_label(world: &dyn World, span: Span) -> Option<Label<FileId>> {
        Some(Label::primary(span.id()?, world.range(span)?))
    }

    let shim = WorldShim { world, root };

    for diagnostic in warnings.iter().chain(errors) {
        let diag = match diagnostic.severity {
            Severity::Error => Diagnostic::error(),
            Severity::Warning => Diagnostic::warning(),
        }
        .with_message(diagnostic.message.clone())
        .with_notes(
            diagnostic
                .hints
                .iter()
                .map(|e| (eco_format!("hint: {e}")).into())
                .collect(),
        )
        .with_labels(resolve_label(world, diagnostic.span).into_iter().collect());

        emit(w, diagnostic_config, &shim, &diag)?;

        // Stacktrace-like helper diagnostics.
        for point in &diagnostic.trace {
            let message = point.v.to_string();
            let help = Diagnostic::help()
                .with_message(message)
                .with_labels(resolve_label(world, point.span).into_iter().collect());

            emit(w, diagnostic_config, &shim, &help)?;
        }
    }

    Ok(())
}

struct WorldShim<'w> {
    world: &'w dyn World,
    root: &'w Path,
}

impl WorldShim<'_> {
    fn lookup(&self, id: FileId) -> Source {
        self.world
            .source(id)
            .expect("world must have source for which it emitted diagnostic")
    }
}

impl<'a> Files<'a> for WorldShim<'_> {
    type FileId = FileId;
    type Name = String;
    type Source = Source;

    fn name(&'a self, id: FileId) -> Result<Self::Name, Error> {
        let vpath = id.vpath();
        Ok(if let Some(package) = id.package() {
            format!("{package}{}", vpath.as_rooted_path().display())
        } else {
            vpath
                .resolve(self.root)
                .unwrap_or_else(|| vpath.as_rootless_path().to_path_buf())
                .to_string_lossy()
                .into()
        })
    }

    fn source(&'a self, id: FileId) -> Result<Self::Source, Error> {
        Ok(self.lookup(id))
    }

    fn line_index(&'a self, id: FileId, given: usize) -> Result<usize, Error> {
        let source = self.lookup(id);
        source
            .byte_to_line(given)
            .ok_or_else(|| Error::IndexTooLarge {
                given,
                max: source.len_bytes(),
            })
    }

    fn line_range(&'a self, id: FileId, given: usize) -> Result<std::ops::Range<usize>, Error> {
        let source = self.lookup(id);
        source
            .line_to_range(given)
            .ok_or_else(|| Error::LineTooLarge {
                given,
                max: source.len_lines(),
            })
    }

    fn column_number(&'a self, id: FileId, _: usize, given: usize) -> Result<usize, Error> {
        let source = self.lookup(id);
        source.byte_to_column(given).ok_or_else(|| {
            let max = source.len_bytes();
            if given <= max {
                Error::InvalidCharBoundary { given }
            } else {
                Error::IndexTooLarge { given, max }
            }
        })
    }
}
