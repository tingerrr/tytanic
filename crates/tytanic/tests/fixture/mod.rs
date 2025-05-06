#![allow(dead_code)]

use std::ffi::OsStr;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::process::{self, ExitStatus};

use assert_cmd::Command;
use tempdir::TempDir;
use tytanic_utils::fs::TEMP_DIR_PREFIX;
use tytanic_utils::result::ResultEx;

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
            dir: TempDir::new(TEMP_DIR_PREFIX).unwrap(),
        }
    }

    /// Creates a new test environment with the default package fixture.
    ///
    /// The package fixture can be found in the repository assets.
    pub fn default_package() -> Self {
        let this = Self::new();
        let fixture = PathBuf::from_iter([
            std::env!("CARGO_MANIFEST_DIR"),
            "..",
            "..",
            "assets",
            "test-package",
        ]);
        copy_dir(&fixture, this.root()).unwrap();
        this
    }
}

impl Environment {
    /// The root of this environment.
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Persists the temporary directory.
    pub fn persist(self) -> PathBuf {
        self.dir.into_path()
    }
}

impl Environment {
    /// Runs tytanic in the test environment.
    pub fn run_tytanic_with<F>(&self, f: F) -> Run
    where
        F: FnOnce(&mut Command) -> &mut Command,
    {
        let mut cmd = Command::cargo_bin("tt").unwrap();
        cmd.current_dir(self.root());

        f(&mut cmd);

        let output = cmd.output().unwrap();

        Run {
            cmd,
            output: Output::from_std_output(output, self.root()),
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
    /// Converts the output into UTF-8 and replaces
    /// - ASCII ESC bytes with `<ESC>` and
    /// - `dir` with `<TEMP_DIR>`.
    fn from_std_output(output: process::Output, dir: &Path) -> Self {
        fn convert_bytes(bytes: Vec<u8>, dir: &str) -> String {
            String::from_utf8(bytes)
                .unwrap()
                .replace("\u{1b}", "<ESC>")
                .replace("\\", "/")
                .replace(dir, "<TEMP_DIR>")
        }

        let dir = dir.as_os_str().to_str().unwrap();

        Output {
            stdout: convert_bytes(output.stdout, dir),
            stderr: convert_bytes(output.stderr, dir),
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
        match self.status.code() {
            Some(code) => writeln!(f, "--- CODE: {}", code)?,
            None => writeln!(f, "--- SIGNALED: This is most likely a bug!")?,
        }
        writeln!(f, "--- STDOUT:")?;
        writeln!(f, "{}", self.stdout)?;
        writeln!(f, "--- STDERR:")?;
        writeln!(f, "{}", self.stderr)?;
        writeln!(f, "--- END")?;

        Ok(())
    }
}

/// This should only be used for copying the test package fixture into a
/// freshly created temporary directory. It assumes no symlinks are present, the
/// `src` exsits and the `dst` does not exist, but its immediate parent does.
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir(dst).ignore(|e| e.kind() == std::io::ErrorKind::AlreadyExists)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;

        let src = entry.path();
        let dst = dst.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            if !std::fs::exists(&dst)? {
                std::fs::write(&dst, "")?;
            }
            std::fs::copy(&src, &dst)?;
        }
    }

    Ok(())
}
