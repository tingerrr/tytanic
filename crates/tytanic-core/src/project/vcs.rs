//! Version control support, this is used in a project to ensure that ephemeral
//! storage directories are not managed by the VCS of the user. Currently
//! supports `.gitignore` and `.hgignore` based VCS' as well as auto discovery
//! of Git, Mercurial and Jujutsu through their hidden repository directories.

use std::fmt::{self, Debug, Display};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::test::Test;

use super::Paths;

/// The name of the git ignore file.
const GITIGNORE_NAME: &str = ".gitignore";

/// The name of the mercurial ignore file.
const HGIGNORE_NAME: &str = ".hgignore";

/// The content of the generated git ignore file.
const IGNORE_HEADER: &str = "# generated by tytanic, do not edit";

/// The kind of [`Vcs`] in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Uses `.gitignore` files to ignore temporary files and directories.
    ///
    /// This means it can also be used by Vcs' which support `.gitignore` files,
    /// like Jujutsu.
    Git,

    /// Uses `.hgignore` files to ignore temporary files and directories.
    ///
    /// This means it can also be used by Vcs' which support `.hgignore` files.
    Mercurial,
}

/// A version control system, this is used to handle persistent storage of
/// reference images and ignoring of non-persistent directories like the `out`
/// and `diff` directories.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vcs {
    root: PathBuf,
    kind: Kind,
}

impl Vcs {
    /// Creates a new Vcs with the given root directory and kind.
    pub fn new<I>(root: I, kind: Kind) -> Self
    where
        I: Into<PathBuf>,
    {
        Self {
            root: root.into(),
            kind,
        }
    }

    /// Checks the given directory for a Vcs, returning it a vcs is rooted here.
    pub fn try_new(root: &Path) -> io::Result<Option<Self>> {
        if root.join(".git").try_exists()? || root.join(".jj").try_exists()? {
            Ok(Some(Self::new(root, Kind::Git)))
        } else if root.join(".hg").try_exists()? {
            Ok(Some(Self::new(root, Kind::Mercurial)))
        } else {
            Ok(None)
        }
    }
}

impl Vcs {
    /// The root of this Vcs' repository.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The kind of this repository.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Ignore all ephemeral files and directories of a test.
    pub fn ignore(&self, paths: &Paths, test: &Test) -> io::Result<()> {
        let mut content = format!("{IGNORE_HEADER}\n\n");

        let file = paths.test_dir(test.id()).join(match self.kind {
            Kind::Git => GITIGNORE_NAME,
            Kind::Mercurial => {
                content.push_str("syntax: glob\n");
                HGIGNORE_NAME
            }
        });

        for always in ["diff/**\n", "out/**\n"] {
            content.push_str(always);
        }

        if !test.kind().is_persistent() {
            content.push_str("ref/**\n");
        }

        fs::write(file, content)?;

        Ok(())
    }

    pub fn unignore(&self, paths: &Paths, test: &Test) -> io::Result<()> {
        let file = paths.test_dir(test.id()).join(match self.kind {
            Kind::Git => GITIGNORE_NAME,
            Kind::Mercurial => HGIGNORE_NAME,
        });

        fs::remove_file(file)
    }
}

impl Display for Vcs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self.kind {
            Kind::Git => "Git",
            Kind::Mercurial => "Mercurial",
        })
    }
}

#[cfg(test)]
mod tests {
    use ecow::eco_vec;

    use super::*;
    use crate::_dev;
    use crate::project::test::Id;
    use crate::project::Paths;
    use crate::test::Kind as TestKind;

    fn test(kind: TestKind) -> Test {
        Test {
            id: Id::new("fancy").unwrap(),
            kind,
            annotations: eco_vec![],
        }
    }

    #[test]
    fn test_git_ignore_create() {
        _dev::fs::TempEnv::run(
            |root| root.setup_dir("tests/fancy"),
            |root| {
                let paths = Paths::new(root, None);
                let vcs = Vcs::new(root, Kind::Git);
                let test = test(TestKind::CompileOnly);
                vcs.ignore(&paths, &test).unwrap();
            },
            |root| {
                root.expect_dir("tests/fancy").expect_file_content(
                    "tests/fancy/.gitignore",
                    format!("{IGNORE_HEADER}\n\ndiff/**\nout/**\nref/**\n"),
                )
            },
        );
    }

    #[test]
    fn test_git_ignore_truncate() {
        _dev::fs::TempEnv::run(
            |root| root.setup_file("tests/fancy/.gitignore", "blah blah"),
            |root| {
                let paths = Paths::new(root, None);
                let vcs = Vcs::new(root, Kind::Git);
                let test = test(TestKind::CompileOnly);
                vcs.ignore(&paths, &test).unwrap();
            },
            |root| {
                root.expect_dir("tests/fancy").expect_file_content(
                    "tests/fancy/.gitignore",
                    format!("{IGNORE_HEADER}\n\ndiff/**\nout/**\nref/**\n"),
                )
            },
        );
    }

    #[test]
    fn test_git_unignore() {
        _dev::fs::TempEnv::run(
            |root| root.setup_file("tests/fancy/.gitignore", "blah blah"),
            |root| {
                let paths = Paths::new(root, None);
                let vcs = Vcs::new(root, Kind::Git);
                let test = test(TestKind::CompileOnly);
                vcs.unignore(&paths, &test).unwrap();
            },
            |root| root.expect_dir("tests/fancy"),
        );
    }
}
