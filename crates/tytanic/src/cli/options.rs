use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::{Args, ColorChoice, Parser, ValueEnum};
use color_eyre::eyre;

use super::{add, list, remove, run, status, update, util, Context};

// TODO(tinger): use built in negation once in clap
// See: https://github.com/clap-rs/clap/issues/815

// TODO(tinger): remove rustdoc attributes once markdown support is in clap stable

/// The separator used for multiple paths.
const ENV_PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// A trait for switches, i.e. options which come in pairs of flags and inverse
/// flags.
pub trait Switch: Sized {
    /// The default value, if no flag was used.
    const DEFAULT: bool;

    /// Return whichever flag was last set.
    fn get(self) -> Option<bool>;

    /// Return whichever flag was last set or the default.
    fn get_or_default(self) -> bool {
        self.get().unwrap_or(Self::DEFAULT)
    }
}

macro_rules! impl_switch {
    (
        $(#[$switch_meta:meta])*
        $switch:ident($default:literal) {
            $(#[$field_meta:meta])*
            $field:ident $(= $field_short:literal)?,

            $(#[$no_field_meta:meta])*
            $no_field:ident $(= $no_field_short:literal)?,
        }
    ) => {
        $(#[$switch_meta])*
        #[derive(Args, Debug, Clone, Copy)]
        pub struct $switch {
            $(#[$field_meta])*
            #[arg(long, global = true)]
            $(#[arg(short = $field_short)])?
            $field: bool,

            $(#[$no_field_meta])*
            #[arg(
                long,
                hide_short_help = true,
                overrides_with = stringify!($field),
                global = true,
            )]
            $(#[arg(short = $no_field_short)])?
            $no_field: bool,
        }

        impl Switch for $switch {
            const DEFAULT: bool = $default;

            fn get(self) -> Option<bool> {
                if self.$field {
                    Some(true)
                } else if self.$no_field {
                    Some(false)
                } else {
                    None
                }
            }
        }
    };
}

impl_switch! {
    /// The `--compare`/`--no-compare` option.
    CompareSwitch(true) {
        #[allow(rustdoc::broken_intra_doc_links)]
        /// Compare tests if they have references [default]
        compare,
        /// Don't compare tests
        no_compare,
    }
}

impl_switch! {
    /// The `--export-ephemeral`/`--no-export-ephemeral` option.
    ExportEphemeralSwitch(true) {
        #[allow(rustdoc::broken_intra_doc_links)]
        /// Export ephemeral documents [default]
        ///
        /// Ephemeral documents are those which are created for each test run,
        /// i.e. non-persistent ones.
        export_ephemeral,
        /// Don't export temporaries
        no_export_ephemeral,
    }
}

impl_switch! {
    /// The `--fail-fast`/`--no-fail-fast` option.
    FailFastSwitch(true) {
        #[allow(rustdoc::broken_intra_doc_links)]
        /// Abort after the first test failure [default]
        fail_fast = 'f',
        /// Don't abort after the first test failure
        no_fail_fast = 'F',
    }
}

impl_switch! {
    /// The `--skip`/`--no-skip` option.
    SkipSwitch(true) {
        #[allow(rustdoc::broken_intra_doc_links)]
        /// Automatically remove skipped tests [default]
        ///
        /// Equivalent to wrapping the test set expression in `(...) ~ skip()`.
        skip = 's',

        /// Don't automatically remove skipped tests
        no_skip = 'S',
    }
}

impl_switch! {
    /// The `--optimize-refs`/`--no-optimize-refs` option.
    OptimizeRefsSwitch(true) {
        #[allow(rustdoc::broken_intra_doc_links)]
        /// Optimize persistent references [default]
        optimize_refs,

        /// Don't optimize persistent references
        no_optimize_refs,
    }
}

macro_rules! ansi {
    ($s:expr; b) => {
        concat!("\x1B[1m", $s, "\x1B[0m")
    };
    ($s:expr; u) => {
        concat!("\x1B[4m", $s, "\x1B[0m")
    };
    ($s:expr;) => {
        $s
    };
    ($s:expr; $first:ident $( + $rest:tt)*) => {
        ansi!(ansi!($s; $($rest)*); $first)
    };
}

// NOTE(tinger): we use clap style formatting here and keep it simple to avoid a
// proc macro dependency for a single use of static ansi formatting
#[rustfmt::skip]
static AFTER_LONG_ABOUT: &str = concat!(
    ansi!("Exit Codes:\n"; u + b),
    "  ", ansi!("0"; b), "  Success\n",
    "  ", ansi!("1"; b), "  At least one test failed\n",
    "  ", ansi!("2"; b), "  The requested operation failed\n",
    "  ", ansi!("3"; b), "  An unexpected error occurred",
);

/// Run and manage tests for typst projects
#[derive(Parser, Debug, Clone)]
#[command(version, after_long_help = AFTER_LONG_ABOUT)]
pub struct CliArguments {
    /// The command to run
    #[command(subcommand)]
    pub cmd: Command,

    #[command(flatten, next_help_heading = "Typst Options")]
    pub typst: TypstOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    pub output: OutputArgs,
}

fn parse_source_date_epoch(raw: &str) -> Result<DateTime<Utc>, String> {
    let timestamp: i64 = raw
        .parse()
        .map_err(|err| format!("timestamp must be decimal integer ({err})"))?;
    DateTime::from_timestamp(timestamp, 0).ok_or_else(|| "timestamp out of range".to_string())
}

/// Options which mirror those of the typst CLI.
///
/// These options are global.
#[derive(Args, Debug, Clone)]
pub struct TypstOptions {
    /// The project root directory
    ///
    /// If none is given, then the first ancestor with a `typst.toml` is used.
    #[arg(long, short, env = "TYPST_ROOT", global = true)]
    pub root: Option<PathBuf>,

    /// The number of threads to use for compilation
    #[arg(long, short, global = true)]
    pub jobs: Option<usize>,

    /// The timestamp used for compilation.
    ///
    /// For more information, see
    /// <https://reproducible-builds.org/specs/source-date-epoch/>.
    #[arg(
        long,
        env = "SOURCE_DATE_EPOCH",
        value_name = "UNIX_TIMESTAMP",
        value_parser = parse_source_date_epoch,
        global = true,
    )]
    pub creation_timestamp: Option<DateTime<Utc>>,

    #[command(flatten)]
    pub font: FontOptions,

    #[command(flatten)]
    pub package: PackageOptions,
}

/// Options for configuring how to load fonts.
///
/// These options are global.
#[derive(Args, Debug, Clone)]
pub struct FontOptions {
    /// Do not read system fonts
    #[arg(long, global = true)]
    pub ignore_system_fonts: bool,

    /// Add a directory to read fonts from (can be repeated)
    #[arg(
        long = "font-path",
        env = "TYPST_FONT_PATHS",
        value_name = "DIR",
        value_delimiter = ENV_PATH_SEP,
        global = true,
    )]
    pub font_paths: Vec<PathBuf>,
}

/// Options for configuring how to store and load packages.
///
/// These options are global.
#[derive(Args, Debug, Clone)]
pub struct PackageOptions {
    /// Custom path to local packages, defaults to system-dependent location
    #[clap(long, env = "TYPST_PACKAGE_PATH", value_name = "DIR", global = true)]
    pub package_path: Option<PathBuf>,

    /// Custom path to package cache, defaults to system-dependent location
    #[clap(
        long,
        env = "TYPST_PACKAGE_CACHE_PATH",
        value_name = "DIR",
        global = true
    )]
    pub package_cache_path: Option<PathBuf>,

    /// Path to a custom CA certificate to use when making network requests
    #[clap(long, visible_alias = "cert", env = "TYPST_CERT", global = true)]
    pub certificate: Option<PathBuf>,
}

/// Options for filtering/selecting tests.
#[derive(Args, Debug, Clone)]
pub struct FilterOptions {
    #[allow(rustdoc::bare_urls)]
    /// A test set expression for filtering tests
    ///
    /// See the language reference and guide a
    /// https://tingerrr.github.io/tytanic/index.html
    /// for more info.
    #[arg(short, long, default_value = "all()", value_name = "EXPR")]
    pub expression: String,

    #[command(flatten)]
    pub skip: SkipSwitch,

    /// The exact tests to operate on
    ///
    /// Implies `--no-skip`. Equivalent to passing
    /// `--expression 'exact:a | exact:b | ...'`.
    #[arg(required = false, conflicts_with = "expression", value_name = "TEST")]
    pub tests: Vec<String>,
}

/// Options for document compilaiton.
#[derive(Args, Debug, Clone)]
pub struct CompileOptions {
    /// How to handle warnings
    #[arg(long, default_value = "emit", value_name = "WHAT")]
    pub warnings: Warnings,
}

/// The options for handling warnings.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Warnings {
    /// Ignore warnings.
    Ignore,

    /// Emit warnigns.
    Emit,

    /// Promote warnings to errors.
    Promote,
}

/// Options for document rendering and export.
#[derive(Args, Debug, Clone)]
pub struct ExportOptions {
    /// The document direction
    ///
    /// This is used to correctly align images with different dimensions when
    /// generating diff images.
    #[arg(long, default_value = "ltr")]
    pub dir: Direction,

    /// The pixel-per-inch value to use for export
    #[arg(long, default_value_t = 144.0)]
    pub ppi: f32,

    #[command(flatten)]
    pub export_ephemeral: ExportEphemeralSwitch,

    #[command(flatten)]
    pub optimize_refs: OptimizeRefsSwitch,
}

/// The reading direction of a document.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// The document is read left-to-right.
    Ltr,

    /// The document is read right-to-left.
    Rtl,
}

/// Options for configuring how to compare output to references.
#[derive(Args, Debug, Clone)]
pub struct CompareOptions {
    #[command(flatten)]
    pub compare: CompareSwitch,

    /// The maximum delta in each channel of a pixel
    ///
    /// If a single channel (red/green/blue/alpha component) of a pixel differs
    /// by this much between reference and output the pixel is counted as a
    /// deviation.
    #[arg(long, default_value_t = 0)]
    pub max_delta: u8,

    /// The maximum deviations per reference
    ///
    /// If a reference and output image have more than the given deviations it's
    /// counted as a failure.
    #[arg(long, default_value_t = 0)]
    pub max_deviations: usize,
}

/// Options for configuring the test runner.
#[derive(Args, Debug, Clone)]
pub struct RunnerOptions {
    #[command(flatten)]
    pub fail_fast: FailFastSwitch,
}

/// Options for configuring the CLI output.
///
/// These options are global.
#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    /// When to use colorful output
    ///
    /// If set to auto, color will only be enabled if a capable terminal is
    /// detected.
    #[clap(
        long,
        value_name = "WHEN",
        require_equals = true,
        num_args = 0..=1,
        default_value = "auto",
        default_missing_value = "always",
        global = true,
    )]
    pub color: ColorChoice,

    /// Produce more logging output [-v ... -vvvvv]
    ///
    /// Logs are written to stderr, the increasing number of verbose flags
    /// corresponds to the log levels ERROR, WARN, INFO, DEBUG, TRACE.
    #[arg(long, short, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Show information about the current project
    #[command(visible_alias = "st")]
    Status(status::Args),

    /// List the tests in the current project
    #[command(visible_alias = "ls")]
    List(list::Args),

    /// Compile and compare tests
    #[command(visible_alias = "r")]
    Run(run::Args),

    /// Compile and update tests
    #[command()]
    Update(update::Args),

    /// Add a new test
    ///
    /// The default test simply contains `Hello World`, if a
    /// test template file is given, it is used instead.
    #[command()]
    Add(add::Args),

    /// Remove tests
    #[command(visible_alias = "rm")]
    Remove(remove::Args),

    /// Utility commands
    #[command()]
    Util(util::Args),
}

impl Command {
    pub fn run(&self, ctx: &mut Context<'_>) -> eyre::Result<()> {
        match self {
            Command::Add(args) => add::run(ctx, args),
            Command::Remove(args) => remove::run(ctx, args),
            Command::Status(args) => status::run(ctx, args),
            Command::List(args) => list::run(ctx, args),
            Command::Update(args) => update::run(ctx, args),
            Command::Run(args) => run::run(ctx, args),
            Command::Util(args) => args.cmd.run(ctx),
        }
    }
}
