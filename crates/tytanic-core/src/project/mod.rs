//! Reading and managing typst projects.

use std::path::{Path, PathBuf};
use std::{fs, io};

use thiserror::Error;
use typst::syntax::package::PackageManifest;
use tytanic_utils::result::ResultEx;

use crate::suite::{Error as SuiteError, Suite};
use crate::test::Id;

mod vcs;

pub use vcs::{Kind as VcsKind, Vcs};

/// The name of the manifest file which is used to discover the project root
/// automatically.
pub const MANIFEST_FILE: &str = "typst.toml";

/// An object which contains various paths relevant for handling on-disk
/// operations and path transformations.
///
/// The paths retruned by this struct are not guaranteed to exist on disk, but
/// if they don't exist at the given paths, then they don't exist for a project.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Paths {
    project: PathBuf,
    vcs: Option<PathBuf>,
}

impl Paths {
    /// Create a new project with the given roots.
    ///
    /// It is recommended to canonicalize them, but it is not strictly necessary.
    pub fn new<P, Q>(project: P, vcs: Q) -> Self
    where
        P: Into<PathBuf>,
        Q: Into<Option<PathBuf>>,
    {
        Self {
            project: project.into(),
            vcs: vcs.into(),
        }
    }
}

impl Paths {
    /// Returns the  path to the project root.
    ///
    /// The project root is used to resolve absolute paths in typst when
    /// executing tests.
    pub fn project_root(&self) -> &Path {
        &self.project
    }

    /// Returns the path to the project manifest (`typst.toml`).
    pub fn manifest(&self) -> PathBuf {
        self.project.join(MANIFEST_FILE)
    }

    /// Returns the path to the test root. That is the path within the project
    /// root where the test suite is located.
    ///
    /// The test root is used to resolve test identifiers.
    pub fn test_root(&self) -> PathBuf {
        self.project.join("tests")
    }

    /// Returns the path to the unit test template, that is, the source template to
    /// use when generating new unit tests.
    ///
    /// See [`Paths::template_dir`] for reading the template.
    pub fn unit_test_template(&self) -> PathBuf {
        self.test_root().join("template.typ")
    }

    /// Returns the absolute canonicalized path to the vcs root. That is the
    /// path within which the project may be located.
    ///
    /// The vcs root is used for properly handling non-persistent storage of
    /// tests.
    pub fn vcs_root(&self) -> Option<&Path> {
        self.vcs.as_deref()
    }

    /// Create a path to the test directory for the given identifier.
    pub fn unit_test_dir(&self, id: &Id) -> PathBuf {
        let mut dir = self.test_root();
        dir.extend(id.components());
        dir
    }

    /// Create a path to the test script for the given identifier.
    pub fn unit_test_script(&self, id: &Id) -> PathBuf {
        let mut dir = self.unit_test_dir(id);
        dir.push("test.typ");
        dir
    }

    /// Create a path to the reference script for the given identifier.
    pub fn unit_test_ref_script(&self, id: &Id) -> PathBuf {
        let mut dir = self.unit_test_dir(id);
        dir.push("ref.typ");
        dir
    }

    /// Create a path to the reference directory for the given identifier.
    pub fn unit_test_ref_dir(&self, id: &Id) -> PathBuf {
        let mut dir = self.unit_test_dir(id);
        dir.push("ref");
        dir
    }

    /// Create a path to the output directory for the given identifier.
    pub fn unit_test_out_dir(&self, id: &Id) -> PathBuf {
        let mut dir = self.unit_test_dir(id);
        dir.push("out");
        dir
    }

    /// Create a path to the difference directory for the given identifier.
    pub fn unit_test_diff_dir(&self, id: &Id) -> PathBuf {
        let mut dir = self.unit_test_dir(id);
        dir.push("diff");
        dir
    }
}

/// A handle for managing typst projects both on-disk and in-memory.
#[derive(Debug, Clone)]
pub struct Project {
    paths: Paths,
    vcs: Option<Vcs>,
}

impl Project {
    /// Create a new project with the given parameters.
    pub fn new(paths: Paths, vcs: Option<Vcs>) -> Self {
        Self { paths, vcs }
    }

    /// Attempt to discover the current project from the given directory.
    ///
    /// If `is_project_root` is `false`, then this will attempt to find it by
    /// looking for a manifest, otherwise it will assume the directory itself is
    /// the project root.
    pub fn discover<P: AsRef<Path>>(
        dir: P,
        is_project_root: bool,
    ) -> Result<Option<Self>, io::Error> {
        let dir = dir.as_ref();

        let mut project_root = is_project_root.then(|| dir.to_path_buf());
        let mut vcs_root = None;
        let mut vcs = None;

        for dir in dir.ancestors() {
            if project_root.is_none() {
                let manifest_file = dir.join(MANIFEST_FILE);
                if manifest_file.try_exists()? {
                    project_root = Some(dir.to_path_buf());
                }
            }

            if vcs.is_none() {
                if let Some(found) = Vcs::try_new(dir)? {
                    tracing::debug!(?found, "found vcs");
                    vcs = Some(found);
                }
                vcs_root = Some(dir.to_path_buf());
            }

            if project_root.is_some() && vcs.is_some() {
                break;
            }
        }

        let Some(project) = project_root else {
            return Ok(None);
        };

        Ok(Some(Self {
            paths: Paths {
                project,
                vcs: vcs_root,
            },
            vcs,
        }))
    }
}

impl Project {
    /// Returns the paths for this project, these are used in various low-level
    /// on-disk operations to correctly manipulate tests.
    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    /// Returns the [`Vcs`] this project is managed by or `None` if no supported
    /// Vcs was found.
    pub fn vcs(&self) -> Option<&Vcs> {
        self.vcs.as_ref()
    }
}

impl Project {
    /// Attempts to read the project manifest if it exists. Returns `None` if no
    /// manifest is found.
    pub fn read_manifest(&self) -> Result<Option<PackageManifest>, ManifestError> {
        Ok(fs::read_to_string(self.paths.manifest())
            .ignore(|e| e.kind() == io::ErrorKind::NotFound)?
            .as_deref()
            .map(toml::from_str)
            .transpose()?)
    }

    /// Collect the full test suite.
    pub fn collect_suite(&self) -> Result<Suite, SuiteError> {
        Suite::collect(&self.paths)
    }
}

/// Returned by [`Project::read_manifest`].
#[derive(Debug, Error)]
pub enum ManifestError {
    /// An error occurred while parsing the project manifest.
    #[error("an error occurred while parsing the project manifest")]
    Parse(#[from] toml::de::Error),

    /// An io error occurred.
    #[error("an io error occurred")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_test_paths() {
        let paths = Paths::new("root", None);
        let id = Id::new("a/b").unwrap();

        assert_eq!(
            paths.unit_test_dir(&id),
            PathBuf::from_iter(["root", "tests", "a", "b"])
        );
        assert_eq!(
            paths.unit_test_script(&id),
            PathBuf::from_iter(["root", "tests", "a", "b", "test.typ"])
        );
        assert_eq!(
            paths.unit_test_ref_script(&id),
            PathBuf::from_iter(["root", "tests", "a", "b", "ref.typ"])
        );
        assert_eq!(
            paths.unit_test_ref_dir(&id),
            PathBuf::from_iter(["root", "tests", "a", "b", "ref"])
        );
        assert_eq!(
            paths.unit_test_out_dir(&id),
            PathBuf::from_iter(["root", "tests", "a", "b", "out"])
        );
        assert_eq!(
            paths.unit_test_diff_dir(&id),
            PathBuf::from_iter(["root", "tests", "a", "b", "diff"])
        );
    }
}
