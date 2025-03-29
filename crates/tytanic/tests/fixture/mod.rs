use std::ffi::OsStr;
use std::process::{ExitStatus, Output};

/// A builder for configuring and running cli tests.
pub struct TestBuilder {
    cmd: assert_cmd::Command,
    dir: assert_fs::TempDir,
}

impl TestBuilder {
    /// Creates a new test environment.
    pub fn new() -> Self {
        let dir = assert_fs::TempDir::new().unwrap();

        let mut cmd = assert_cmd::Command::cargo_bin("tt").unwrap();
        cmd.current_dir(dir.path());

        Self { cmd, dir }
    }

    /// Creates a new test environment with the given arguments.
    pub fn new_with_args<I, T>(args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: AsRef<OsStr>,
    {
        Self::new().with_cmd(|cmd| cmd.args(args))
    }

    /// Configure the command to use.
    pub fn with_cmd<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut assert_cmd::Command) -> &mut assert_cmd::Command,
    {
        f(&mut self.cmd);
        self
    }
}

impl TestBuilder {
    /// Run the test and return the result.
    pub fn run(mut self) -> TestResult {
        let output = self.cmd.output().unwrap();

        TestResult {
            cmd: self.cmd,
            dir: self.dir,
            output: TestOutput {
                stdout: String::from_utf8(output.stdout).unwrap(),
                stderr: String::from_utf8(output.stderr).unwrap(),
                status: output.status,
            },
        }
    }
}

/// The result of a test run.
pub struct TestResult {
    cmd: assert_cmd::Command,
    dir: assert_fs::TempDir,
    output: TestOutput,
}

impl TestResult {
    /// The command used for this test run.
    pub fn cmd(&self) -> &assert_cmd::Command {
        &self.cmd
    }

    /// The temp dir used for this test run.
    pub fn dir(&self) -> &assert_fs::TempDir {
        &self.dir
    }

    /// The output of this test run.
    pub fn output(&self) -> &TestOutput {
        &self.output
    }
}

/// The output of running the `tt` binary.
pub struct TestOutput {
    stdout: String,
    stderr: String,
    status: ExitStatus,
}
impl TestOutput {
    /// The stdout of the run.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// The stderr of the run.
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// The status of the run.
    pub fn status(&self) -> ExitStatus {
        self.status
    }
}
