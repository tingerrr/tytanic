use std::ffi::OsStr;
use std::fmt::Display;
use std::path::Path;
use std::process::{self, ExitStatus};

use assert_cmd::Command;
use assert_fs::TempDir;
use tytanic_utils::typst::PackageManifestBuilder;

// NOTE(tinger): We don't do any fancy error handling here because this is
// exclusively used for tests.

// TODO(tinger): Add configuration options and presets for project
// configurations such as tests and configurations.

/// A test environment in which to execute tytanic.
#[derive(Debug)]
pub struct Environment {
    dir: TempDir,
}

impl Environment {
    /// Creates a new empty test environment.
    pub fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

    /// Creates a new test environment with the given manifest.
    pub fn new_with<F>(manifest: F) -> Self
    where
        F: FnOnce(&mut PackageManifestBuilder) -> &mut PackageManifestBuilder,
    {
        let this = Self::new();

        let manifest = {
            let mut builder = PackageManifestBuilder::new();
            manifest(&mut builder);

            builder.build()
        };

        std::fs::write(
            this.dir.path().join("typst.toml"),
            toml::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        this
    }

    /// Creates a new test environment with an empty package.
    pub fn empty_package() -> Self {
        Self::new_with(|m| m)
    }
}

impl Environment {
    /// The temp dir used for this environment.
    pub fn dir(&self) -> &TempDir {
        &self.dir
    }

    /// The root of this environment.
    pub fn root(&self) -> &Path {
        self.dir.path()
    }
}

impl Environment {
    /// Runs tytanic in the test environment.
    pub fn run_tytanic_with<F>(&self, f: F) -> Run
    where
        F: FnOnce(&mut Command) -> &mut Command,
    {
        let mut cmd = Command::cargo_bin("tt").unwrap();
        cmd.current_dir(self.dir.path());

        f(&mut cmd);

        let output = cmd.output().unwrap();

        Run {
            cmd,
            output: Output::from_std_output(output),
        }
    }

    /// Runs tytanic in the test environment with the given args.
    pub fn run_tytanic<I, T>(&self, args: I) -> Run
    where
        I: IntoIterator<Item = T>,
        T: AsRef<OsStr>,
    {
        self.run_tytanic_with(|cmd| cmd.args(args))
    }

    /// Runs tytanic in the sub directory of the test environment with the given
    /// args.
    pub fn run_tytanic_in<I, T, P>(&self, path: P, args: I) -> Run
    where
        P: AsRef<Path>,
        I: IntoIterator<Item = T>,
        T: AsRef<OsStr>,
    {
        self.run_tytanic_with(|cmd| cmd.current_dir(self.root().join(path)).args(args))
    }
}

/// The result of a run.
#[derive(Debug)]
pub struct Run {
    cmd: Command,
    output: Output,
}

impl Run {
    /// The command used for this run.
    pub fn cmd(&self) -> &Command {
        &self.cmd
    }

    /// The output of this run.
    pub fn output(&self) -> &Output {
        &self.output
    }
}

/// The output of running tytanic.
#[derive(Debug)]
pub struct Output {
    stdout: String,
    stderr: String,
    status: ExitStatus,
}

impl Output {
    /// Converts the output into UTF-8 and removes ANSI escapes.
    fn from_std_output(output: process::Output) -> Self {
        fn convert_bytes(bytes: Vec<u8>) -> String {
            String::from_utf8(bytes).unwrap().replace("\u{1b}", "<ESC>")
        }

        Output {
            stdout: convert_bytes(output.stdout),
            stderr: convert_bytes(output.stderr),
            status: output.status,
        }
    }
}

impl Output {
    /// The sanitized stdout of the run.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// The sanitized stderr of the run.
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// The exit status of the run.
    pub fn status(&self) -> ExitStatus {
        self.status
    }
}

impl Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "--- CODE: {}", self.status)?;
        writeln!(f, "--- STDOUT:")?;
        writeln!(f, "{}", self.stdout)?;
        writeln!(f, "--- STDERR:")?;
        writeln!(f, "{}", self.stderr)?;
        writeln!(f, "--- END")?;

        Ok(())
    }
}
