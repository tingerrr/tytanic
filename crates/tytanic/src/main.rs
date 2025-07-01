//! A test runner for Typst set suites.

use std::env;
use std::io;
use std::io::Write;
use std::process::ExitCode;
use std::sync::atomic::Ordering;

use clap::Parser;
use codespan_reporting::term;
use color_eyre::eyre;
use once_cell::sync::Lazy;
use termcolor::Color;
use termcolor::StandardStream;
use termcolor::WriteColor;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_tree::HierarchicalLayer;

use crate::cli::commands::CliArguments;
use crate::cli::Context;
use crate::cli::OperationFailure;
use crate::cli::TestFailure;
use crate::ui::Ui;

mod cli;
mod json;
mod report;
mod runner;
mod ui;
mod world;

/// The default optimization options to use.
pub static DEFAULT_OPTIMIZE_OPTIONS: Lazy<oxipng::Options> =
    Lazy::new(oxipng::Options::max_compression);

fn main() -> ExitCode {
    match main_impl() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err:?}");
            ExitCode::from(cli::EXIT_ERROR)
        }
    }
}

fn main_impl() -> eyre::Result<ExitCode> {
    let args = CliArguments::parse();

    color_eyre::install()?;

    let cc = match args.output.color {
        clap::ColorChoice::Auto => termcolor::ColorChoice::Auto,
        clap::ColorChoice::Always => termcolor::ColorChoice::Always,
        clap::ColorChoice::Never => termcolor::ColorChoice::Never,
    };

    let ui = Ui::new(
        cc,
        cc,
        term::Config {
            display_style: term::DisplayStyle::Rich,
            tab_width: 2,
            ..Default::default()
        },
    );

    // Emit deprecation warning for aliases
    if let Some(arg) = env::args().skip(1).find(|arg| !arg.starts_with('-')) {
        match &arg[..] {
            "remove" | "rm" => {
                let mut w = ui.warn()?;
                write!(w, "Sub command alias ")?;
                cwrite!(colored(w, Color::Cyan), "remove")?;
                write!(w, "|")?;
                cwrite!(colored(w, Color::Cyan), "rm")?;
                writeln!(w, " is deprecated")?;
                drop(w);

                let mut w = ui.hint()?;
                write!(w, "Use ")?;
                cwrite!(colored(w, Color::Cyan), "delete")?;
                writeln!(w, " instead")?;
            }
            "add" => {
                let mut w = ui.warn()?;
                write!(w, "Sub command alias ")?;
                cwrite!(colored(w, Color::Cyan), "add")?;
                writeln!(w, " is deprecated")?;
                drop(w);

                let mut w = ui.hint()?;
                write!(w, "Use ")?;
                cwrite!(colored(w, Color::Cyan), "new")?;
                writeln!(w, " instead")?;
            }
            _ => {}
        }
    }

    // This is a hack, termcolor does not expose any way for us to easily reuse
    // their internal mechanism of checking whether the given stream is color
    // capable without constructing a stream and asking for it.
    let tracing_ansi = StandardStream::stderr(cc).supports_color();

    tracing_subscriber::registry()
        .with(
            // We set with_ansi to true, because UI handles the usage of color
            // through termcolor::StandardStream.
            HierarchicalLayer::new(4)
                .with_targets(true)
                .with_ansi(tracing_ansi),
        )
        .with(Targets::new().with_target(
            tytanic_core::TOOL_NAME,
            match args.output.verbose {
                0 => LevelFilter::OFF,
                1 => LevelFilter::ERROR,
                2 => LevelFilter::WARN,
                3 => LevelFilter::INFO,
                4 => LevelFilter::DEBUG,
                5.. => LevelFilter::TRACE,
            },
        ))
        .init();

    if let Err(err) = ctrlc::set_handler(|| {
        cli::CANCELLED.store(true, Ordering::SeqCst);
    }) {
        writeln!(ui.error()?, "couldn't register ctrl-c handler:\n{err}")?;
        writeln!(
            ui.hint()?,
            "pressing ctrl-c will discard output of failed tests"
        )?;
    }

    if let Some(jobs) = args.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build_global()
            .ok();
    }

    let mut ctx = Context::new(&args, &ui);

    let exit_code = match ctx.run() {
        Ok(()) => cli::EXIT_OK,
        Err(err) => 'err: {
            let root = err.root_cause();

            for cause in err.chain() {
                if let Some(TestFailure) = cause.downcast_ref() {
                    break 'err cli::EXIT_TEST_FAILURE;
                }

                if let Some(OperationFailure) = cause.downcast_ref() {
                    break 'err cli::EXIT_OPERATION_FAILURE;
                }
            }

            // FIXME(tinger): https://github.com/serde-rs/json/issues/1169
            if root
                .downcast_ref()
                .and_then(serde_json::Error::io_error_kind)
                .or_else(|| root.downcast_ref().map(io::Error::kind))
                .is_some_and(|kind| kind == io::ErrorKind::BrokenPipe)
            {
                break 'err cli::EXIT_OK;
            }

            writeln!(ctx.ui.error()?, "{err:?}")?;
            let mut w = ctx.ui.hint()?;
            writeln!(
                w,
                "Tytanic ran into an unexpected error, this is most likely a bug"
            )?;
            writeln!(
                w,
                "Please consider reporting this at {}/issues/new?template=bug_report.md",
                std::env!("CARGO_PKG_REPOSITORY")
            )?;

            cli::EXIT_OPERATION_FAILURE
        }
    };

    ctx.ui.flush()?;

    Ok(ExitCode::from(exit_code))
}
