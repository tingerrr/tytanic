#![allow(dead_code)]

use std::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::io;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::Stdin;
use std::io::StdinLock;
use std::io::Write;

use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::diagnostic::Label;
use codespan_reporting::term;
use color_eyre::eyre;
use ecow::eco_format;
use termcolor::Color;
use termcolor::ColorChoice;
use termcolor::ColorSpec;
use termcolor::HyperlinkSpec;
use termcolor::StandardStream;
use termcolor::StandardStreamLock;
use termcolor::WriteColor;
use typst::diag::Severity;
use typst::diag::SourceDiagnostic;
use typst::World;
use typst::WorldExt;
use typst_syntax::FileId;
use typst_syntax::Source;
use typst_syntax::Span;
use tytanic_core::test::Id;

#[macro_export]
macro_rules! cwrite {
    ($ctor:ident($dst:expr $(, $($arg1:tt)*)?), $($arg2:tt)*) => {{
        let mut w = $crate::ui::$ctor(&mut $dst $(, $($arg1)*)?)?;
        write!(w, $($arg2)*)?;
        $crate::ui::CWrite::finish(w).map(|_| ())
    }};
}

#[macro_export]
macro_rules! cwriteln {
    ($ctor:ident($dst:expr $(, $($arg1:tt)*)?), $($arg2:tt)*) => {{
        let mut w = $crate::ui::$ctor(&mut $dst $(, $($arg1)*)?)?;
        write!(w, $($arg2)*)?;
        let w = $crate::ui::CWrite::finish(w)?;
        writeln!(w)?;
        ::std::io::Result::Ok(())
    }};
}

pub trait CWrite: WriteColor {
    type Inner;

    fn finish(self) -> io::Result<Self::Inner>;
}

impl CWrite for StandardStreamLock<'_> {
    type Inner = Self;

    fn finish(self) -> io::Result<Self::Inner> {
        Ok(self)
    }
}

/// A terminal ui wrapper for common tasks such as input prompts and output
/// messaging.
#[derive(Debug)]
pub struct Ui {
    /// The unlocked stdin stream.
    stdin: Stdin,

    /// The unlocked stdout stream.
    stdout: StandardStream,

    /// The unlocked stderr stream.
    stderr: StandardStream,

    /// The diagnostic config to use for emitting typst source diagnostics.
    diagnostic_config: term::Config,
}

/// Returns whether or not a given output stream is connected to a terminal.
pub fn check_terminal<T: IsTerminal>(t: T, choice: ColorChoice) -> ColorChoice {
    match choice {
        // When we use auto and the stream is not a terminal, we disable it
        // since termcolor does not check for this, in any other case we let
        // termcolor figure out what to do.
        ColorChoice::Auto if !t.is_terminal() => ColorChoice::Never,
        other => other,
    }
}

impl Ui {
    /// Creates a new [`Ui`] with the gven color choices for stdout and stderr.
    pub fn new(out: ColorChoice, err: ColorChoice, diagnostic_config: term::Config) -> Self {
        Self {
            stdin: io::stdin(),
            stdout: StandardStream::stdout(check_terminal(io::stdout(), out)),
            stderr: StandardStream::stderr(check_terminal(io::stderr(), err)),
            diagnostic_config,
        }
    }
}

impl Ui {
    /// Whether a live status report can be printed and cleared using ANSI
    /// escape codes.
    pub fn can_live_report(&self) -> bool {
        io::stderr().is_terminal()
    }

    /// Whether a prompt can be displayed and confirmed by the user.
    pub fn can_prompt(&self) -> bool {
        io::stdin().is_terminal() && io::stderr().is_terminal()
    }

    /// Returns the diagnostic config to use for displaying diagnostics.
    pub fn diagnostic_config(&self) -> &term::Config {
        &self.diagnostic_config
    }

    /// Returns an exclusive lock to stdin.
    pub fn stdin(&self) -> StdinLock<'_> {
        self.stdin.lock()
    }

    /// Returns an exclusive lock to stdout.
    pub fn stdout(&self) -> StandardStreamLock<'_> {
        self.stdout.lock()
    }

    /// Returns an exclusive lock to stderr.
    pub fn stderr(&self) -> StandardStreamLock<'_> {
        self.stderr.lock()
    }
}

impl Ui {
    /// Returns a writer for emitting a user-facing error.
    pub fn error(&self) -> io::Result<Indented<impl WriteColor + '_>> {
        error(self.stderr())
    }

    /// Returns a writer for emitting a user-facing warning.
    pub fn warn(&self) -> io::Result<Indented<impl WriteColor + '_>> {
        warn(self.stderr())
    }

    /// Returns a writer for emitting a user-facing hint.
    pub fn hint(&self) -> io::Result<Indented<impl WriteColor + '_>> {
        hint(self.stderr())
    }

    /// Prompts the user for input with the given prompt on stderr.
    pub fn prompt_with(
        &self,
        prompt: impl FnOnce(&mut dyn WriteColor) -> io::Result<()>,
    ) -> eyre::Result<String> {
        if !self.can_prompt() {
            eyre::bail!(io::Error::new(
                io::ErrorKind::Unsupported,
                "Cannot prompt for input since the output is not connected to a terminal",
            ));
        }

        let mut stderr = self.stderr();
        let mut stdin = self.stdin();

        prompt(&mut stderr)?;
        stderr.flush()?;

        let mut buffer = String::new();
        stdin.read_line(&mut buffer)?;

        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            eyre::bail!(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Prompt cancelled by EOF",
            ));
        }

        Ok(trimmed.to_owned())
    }

    /// A shorthand for [`Ui::prompt_with`] for confirmations.
    pub fn prompt_yes_no(
        &self,
        prompt: impl Display,
        default: impl Into<Option<bool>>,
    ) -> eyre::Result<bool> {
        let default = default.into();
        let def = match default {
            Some(true) => "Y/n",
            Some(false) => "y/N",
            None => "y/n",
        };

        let res = self.prompt_with(|err| write!(err, "{prompt} [{def}]: "))?;

        Ok(match &res[..] {
            "" => default.ok_or_else(|| eyre::eyre!("expected [y]es or [n]o, got nothing"))?,
            "y" | "Y" => true,
            "n" | "N" => false,
            _ => {
                if res.eq_ignore_ascii_case("yes") {
                    true
                } else if res.eq_ignore_ascii_case("no") {
                    false
                } else {
                    eyre::bail!("expected [y]es or [n]o, got: {res:?}");
                }
            }
        })
    }

    /// Flushes and resets both output streams.
    pub fn flush(&self) -> io::Result<()> {
        let mut out = self.stdout();
        let mut err = self.stderr();

        out.reset()?;
        write!(out, "")?;

        err.reset()?;
        write!(err, "")?;

        Ok(())
    }
}

/// Returns a writer for styled output.
pub fn styled<W, F, G>(w: W, set: F, unset: G) -> io::Result<Styled<W, F, G>>
where
    W: WriteColor,
    F: FnOnce() -> ColorSpec,
    G: FnOnce() -> ColorSpec,
{
    Ok(Styled::new(w, set, unset))
}

/// Returns an italic writer.
pub fn italic<W: WriteColor>(w: W) -> io::Result<impl CWrite<Inner = W>> {
    styled(
        w,
        || {
            let mut spec = ColorSpec::default();
            spec.set_italic(true);
            spec
        },
        || {
            let mut spec = ColorSpec::default();
            spec.set_italic(false);
            spec
        },
    )
}

/// Returns a bold writer.
pub fn bold<W: WriteColor>(w: W) -> io::Result<impl CWrite<Inner = W>> {
    styled(
        w,
        || {
            let mut spec = ColorSpec::default();
            spec.set_bold(true);
            spec
        },
        || {
            let mut spec = ColorSpec::default();
            spec.set_bold(false);
            spec
        },
    )
}

/// Returns a colored writer.
pub fn colored<W: WriteColor>(w: W, color: Color) -> io::Result<impl CWrite<Inner = W>> {
    styled(
        w,
        move || {
            let mut spec = ColorSpec::default();
            spec.set_fg(Some(color));
            spec
        },
        || {
            let mut spec = ColorSpec::default();
            spec.set_fg(None);
            spec
        },
    )
}

/// Returns a colored writer.
pub fn bold_colored<W: WriteColor>(w: W, color: Color) -> io::Result<impl CWrite<Inner = W>> {
    styled(
        w,
        move || {
            let mut spec = ColorSpec::default();
            spec.set_bold(true).set_fg(Some(color));
            spec
        },
        || {
            let mut spec = ColorSpec::default();
            spec.set_bold(false).set_fg(None);
            spec
        },
    )
}

/// Returns a writer for annotated output. Annotated output is output which uses
/// a hanging indent after an initial indentation. The writer will continue on
/// the same line as the annotation.
pub fn annotated<W: WriteColor>(
    mut w: W,
    header: &str,
    color: Color,
    max_align: impl Into<Option<usize>>,
) -> io::Result<Indented<W>> {
    let align = max_align.into().unwrap_or(header.len());
    cwrite!(bold_colored(w, color), "{header:>align$} ")?;

    // When taking the indent from the header length, we need to account for the
    // additional space.
    Ok(Indented::continued(w, align + 1))
}

/// Returns a writer for emitting a user-facing error.
pub fn error<W: WriteColor>(w: W) -> io::Result<Indented<W>> {
    annotated(w, "error:", Color::Red, None)
}

/// Returns a writer for emitting a user-facing warning.
pub fn warn<W: WriteColor>(w: W) -> io::Result<Indented<W>> {
    annotated(w, "warning:", Color::Yellow, None)
}

/// Returns a writer for emitting a user-facing hint.
pub fn hint<W: WriteColor>(w: W) -> io::Result<Indented<W>> {
    annotated(w, "hint:", Color::Cyan, None)
}

/// Write a test id.
pub fn write_test_id(mut w: &mut dyn WriteColor, id: &Id) -> io::Result<()> {
    if !id.module().is_empty() {
        cwrite!(colored(w, Color::Cyan), "{}/", id.module())?;
    }

    cwrite!(bold_colored(w, Color::Blue), "{}", id.name())?;

    Ok(())
}

/// Writes the given diagnostics.
pub fn write_diagnostics(
    w: &mut dyn WriteColor,
    diagnostic_config: &term::Config,
    world: &dyn World,
    warnings: &[SourceDiagnostic],
    errors: &[SourceDiagnostic],
) -> eyre::Result<()> {
    fn resolve_label(world: &dyn World, span: Span) -> Option<Label<FileId>> {
        Some(Label::primary(span.id()?, world.range(span)?))
    }

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

        term::emit(w, diagnostic_config, &WorldShim(world), &diag)?;

        // Stacktrace-like helper diagnostics.
        for point in &diagnostic.trace {
            let message = point.v.to_string();
            let help = Diagnostic::help()
                .with_message(message)
                .with_labels(resolve_label(world, point.span).into_iter().collect());

            term::emit(w, diagnostic_config, &WorldShim(world), &help)?;
        }
    }

    Ok(())
}

struct WorldShim<'w>(&'w dyn World);

impl WorldShim<'_> {
    fn lookup(&self, id: FileId) -> Source {
        self.0.source(id).unwrap()
    }
}

type CodespanResult<T> = Result<T, CodespanError>;
type CodespanError = codespan_reporting::files::Error;

impl<'a> codespan_reporting::files::Files<'a> for WorldShim<'_> {
    type FileId = FileId;
    type Name = String;
    type Source = Source;

    fn name(&'a self, id: FileId) -> CodespanResult<Self::Name> {
        let vpath = id.vpath();
        Ok(if let Some(package) = id.package() {
            format!("{package}{}", vpath.as_rooted_path().display())
        } else {
            // Try to express the path relative to the working directory.
            vpath
                // .resolve(&self.root)
                // .and_then(|abs| pathdiff::diff_paths(abs, self.workdir()))
                // .as_deref()
                // .unwrap_or_else(|| vpath.as_rootless_path().to_path_buf())
                .as_rooted_path()
                .to_string_lossy()
                .into()
        })
    }

    fn source(&'a self, id: FileId) -> CodespanResult<Self::Source> {
        Ok(self.lookup(id))
    }

    fn line_index(&'a self, id: FileId, given: usize) -> CodespanResult<usize> {
        let source = self.lookup(id);
        source
            .byte_to_line(given)
            .ok_or_else(|| CodespanError::IndexTooLarge {
                given,
                max: source.len_bytes(),
            })
    }

    fn line_range(&'a self, id: FileId, given: usize) -> CodespanResult<std::ops::Range<usize>> {
        let source = self.lookup(id);
        source
            .line_to_range(given)
            .ok_or_else(|| CodespanError::LineTooLarge {
                given,
                max: source.len_lines(),
            })
    }

    fn column_number(&'a self, id: FileId, _: usize, given: usize) -> CodespanResult<usize> {
        let source = self.lookup(id);
        source.byte_to_column(given).ok_or_else(|| {
            let max = source.len_bytes();
            if given <= max {
                CodespanError::InvalidCharBoundary { given }
            } else {
                CodespanError::IndexTooLarge { given, max }
            }
        })
    }
}

/// Writes content with some styles, this does not implement [`WriteColor`]
/// because it sets and unsets its own style, manually interference should be
/// avoided.
#[derive(Debug)]
pub struct Styled<W, F, G> {
    /// The writer to write to.
    writer: W,

    /// The set closure.
    set: Option<F>,

    /// The unset closure.
    unset: Option<G>,
}

impl<W, F, G> Styled<W, F, G> {
    /// Creates a new writer which writes with a set of styles.
    pub fn new(writer: W, set: F, unset: G) -> Self {
        Self {
            writer,
            set: Some(set),
            unset: Some(unset),
        }
    }

    /// Returns a mutable reference to the inner writer.
    pub fn inner(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Returns the inner writer without writing the styles.
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: WriteColor, F, G> fmt::Write for Styled<W, F, G>
where
    F: FnOnce() -> ColorSpec,
{
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_all(s.as_bytes()).map_err(|_| fmt::Error)
    }
}

impl<W: WriteColor, F, G> Write for Styled<W, F, G>
where
    F: FnOnce() -> ColorSpec,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all(buf).map(|_| buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        if let Some(set) = self.set.take() {
            self.writer.set_color(&set())?;
        }

        self.writer.write_all(buf)
    }
}

impl<W: WriteColor, F, G> WriteColor for Styled<W, F, G>
where
    F: FnOnce() -> ColorSpec,
{
    fn supports_color(&self) -> bool {
        self.writer.supports_color()
    }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        self.writer.set_color(spec)
    }

    fn reset(&mut self) -> io::Result<()> {
        self.writer.reset()
    }

    fn is_synchronous(&self) -> bool {
        self.writer.is_synchronous()
    }

    fn set_hyperlink(&mut self, link: &HyperlinkSpec) -> io::Result<()> {
        self.writer.set_hyperlink(link)
    }

    fn supports_hyperlinks(&self) -> bool {
        self.writer.supports_hyperlinks()
    }
}

impl<W, F, G> CWrite for Styled<W, F, G>
where
    W: WriteColor,
    F: FnOnce() -> ColorSpec,
    G: FnOnce() -> ColorSpec,
{
    type Inner = W;

    fn finish(mut self) -> io::Result<W> {
        self.writer
            .set_color(&self.unset.take().expect("is only taken once")())?;
        Ok(self.writer)
    }
}

/// Writes content indented, ensuring color specs are correctly enabled and
/// disabled.
#[derive(Debug)]
pub struct Indented<W> {
    /// The writer to write to.
    writer: W,

    /// The current indent.
    indent: usize,

    /// Whether an indent is required at the next newline.
    need_indent: bool,

    /// The color spec to reactivate after the next indent.
    spec: Option<ColorSpec>,
}

impl<W> Indented<W> {
    /// Creates a new writer which indents every non-empty line.
    pub fn new(writer: W, indent: usize) -> Self {
        Self {
            writer,
            indent,
            need_indent: true,
            spec: None,
        }
    }

    /// Creates a new writer which indents every non-empty line after the first
    /// one. This is useful for writers which start on a non-empty line.
    pub fn continued(writer: W, indent: usize) -> Self {
        Self {
            writer,
            indent,
            need_indent: false,
            spec: None,
        }
    }

    /// Returns a mutable reference to the inner writer.
    pub fn inner(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Executes the given closure with an additional indent which is later reset.
    pub fn write_with<R>(&mut self, indent: usize, f: impl FnOnce(&mut Self) -> R) -> R {
        self.indent += indent;
        let res = f(self);
        self.indent -= indent;
        res
    }

    /// Returns the inner writer.
    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Returns the inner writer.
    pub fn finish(self) -> io::Result<W> {
        Ok(self.writer)
    }
}

impl<W: WriteColor> fmt::Write for Indented<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_all(s.as_bytes()).map_err(|_| fmt::Error)
    }
}

impl<W: WriteColor> Write for Indented<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all(buf).map(|_| buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    fn write_all(&mut self, mut buf: &[u8]) -> io::Result<()> {
        let pad = " ".repeat(self.indent);

        loop {
            if self.need_indent {
                match buf.iter().position(|&b| b != b'\n') {
                    None => break self.writer.write_all(buf),
                    Some(len) => {
                        let (head, tail) = buf.split_at(len);
                        self.writer.write_all(head)?;
                        if self.spec.is_some() {
                            self.writer.reset()?;
                        }
                        self.writer.write_all(pad.as_bytes())?;
                        if let Some(spec) = &self.spec {
                            self.writer.set_color(spec)?;
                        }
                        self.need_indent = false;
                        buf = tail;
                    }
                }
            } else {
                match buf.iter().position(|&b| b == b'\n') {
                    None => break self.writer.write_all(buf),
                    Some(len) => {
                        let (head, tail) = buf.split_at(len + 1);
                        self.writer.write_all(head)?;
                        self.need_indent = true;
                        buf = tail;
                    }
                }
            }
        }
    }
}

impl<W: WriteColor> WriteColor for Indented<W> {
    fn supports_color(&self) -> bool {
        self.writer.supports_color()
    }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        self.spec = Some(spec.clone());
        self.writer.set_color(spec)
    }

    fn reset(&mut self) -> io::Result<()> {
        self.spec = None;
        self.writer.reset()
    }

    fn is_synchronous(&self) -> bool {
        self.writer.is_synchronous()
    }

    fn set_hyperlink(&mut self, link: &HyperlinkSpec) -> io::Result<()> {
        self.writer.set_hyperlink(link)
    }

    fn supports_hyperlinks(&self) -> bool {
        self.writer.supports_hyperlinks()
    }
}

impl<W: WriteColor> CWrite for Indented<W> {
    type Inner = W;

    fn finish(self) -> io::Result<W> {
        Ok(self.writer)
    }
}

/// Ensure Ui is thread safe.
#[allow(dead_code)]
fn assert_traits() {
    tytanic_utils::assert::send::<Ui>();
    tytanic_utils::assert::sync::<Ui>();
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use termcolor::Ansi;

    use super::*;

    #[test]
    fn test_indented() {
        let mut w = Indented::new(Ansi::new(vec![]), 2);

        write!(w, "Hello\n\nWorld\n").unwrap();

        let w = w.into_inner().into_inner();
        let str = std::str::from_utf8(&w).unwrap();
        assert_snapshot!(str);
    }

    #[test]
    fn test_indented_continued() {
        let mut w = Indented::continued(Ansi::new(vec![]), 2);

        write!(w, "Hello\n\nWorld\n").unwrap();

        let w = w.into_inner().into_inner();
        let str = std::str::from_utf8(&w).unwrap();
        assert_snapshot!(str);
    }

    #[test]
    fn test_indented_nested() {
        let mut w = Indented::new(Indented::new(Ansi::new(vec![]), 2), 2);

        write!(w, "Hello\n\nWorld\n").unwrap();

        let w = w.into_inner().into_inner().into_inner();
        let str = std::str::from_utf8(&w).unwrap();
        assert_snapshot!(str);
    }

    #[test]
    fn test_indented_set_color() {
        let mut w = Indented::new(Ansi::new(vec![]), 2);

        w.set_color(ColorSpec::new().set_bold(true)).unwrap();
        write!(w, "Hello\n\nWorld\n").unwrap();

        let w = w.into_inner().into_inner();
        let str = std::str::from_utf8(&w).unwrap();
        assert_snapshot!(str);
    }
}
