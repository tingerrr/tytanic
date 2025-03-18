//! Discovering, loading and managing typst projects.

use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::{fs, io};

use ecow::EcoString;
use serde::Deserialize;
use thiserror::Error;
use typst::syntax::package::{PackageManifest, PackageSpec};
use tytanic_utils::result::{io_not_found, ResultEx};

use crate::config::ProjectConfig;
use crate::test::Id;
use crate::TOOL_NAME;

mod vcs;

pub use vcs::{Kind as VcsKind, Vcs};

/// The name of the manifest file which is used to discover the project root
/// automatically.
pub const MANIFEST_FILE: &str = "typst.toml";

/// Represents a "shallow" unloaded project, it contains the base paths required
/// to to load a project.
#[derive(Debug, Clone)]
pub struct ShallowProject {
    root: PathBuf,
    vcs: Option<Vcs>,
}

impl ShallowProject {
    /// Create a new project with the given roots.
    ///
    /// It is recommended to canonicalize them, but it is not strictly necessary.
    pub fn new<P, V>(project: P, vcs: V) -> Self
    where
        P: Into<PathBuf>,
        V: Into<Option<Vcs>>,
    {
        Self {
            root: project.into(),
            vcs: vcs.into(),
        }
    }

    /// Attempt to discover various paths for a directory.
    ///
    /// If `search_manifest` is `true`, then this will attempt to find the
    /// project root by looking for a Typst manifest and return `None` if no
    /// manifest is found. If it is `true`, then `dir` is used as the project
    /// root.
    #[tracing::instrument(skip(dir) fields(dir = ?dir.as_ref()), ret)]
    pub fn discover<P: AsRef<Path>>(
        dir: P,
        search_manifest: bool,
    ) -> Result<Option<Self>, io::Error> {
        let dir = dir.as_ref();

        let mut project = search_manifest.then(|| dir.to_path_buf());
        let mut vcs = None;

        for dir in dir.ancestors() {
            if project.is_none() && Project::exists_at(dir)? {
                tracing::debug!(project_root = ?dir, "found project");
                project = Some(dir.to_path_buf());
            }

            // TODO(tinger): Currently we keep searching for a project even when
            // we find a vcs root, I'm not sure if this makes sense, stopping at
            // the vcs root is likely the most sensible behavior.
            if vcs.is_none() {
                if let Some(kind) = Vcs::exists_at(dir)? {
                    tracing::debug!(vcs = ?kind, root = ?dir, "found vcs");
                    vcs = Some(Vcs::new(dir.to_path_buf(), kind));
                }
            }

            if project.is_some() && vcs.is_some() {
                break;
            }
        }

        let Some(project) = project else {
            return Ok(None);
        };

        Ok(Some(Self { root: project, vcs }))
    }
}

impl ShallowProject {
    /// Loads the manifest, configuration and unit test template of a project.
    #[tracing::instrument]
    pub fn load(self) -> Result<Project, LoadError> {
        let manifest = self.parse_manifest()?;
        let config = manifest
            .as_ref()
            .map(|m| self.parse_config(m))
            .transpose()?
            .flatten()
            .unwrap_or_default();

        let unit_test_template = self.read_unit_test_template(&config)?;

        Ok(Project {
            base: self,
            manifest,
            config,
            unit_test_template,
        })
    }

    /// Parses the project manifest if it exists. Returns `None` if no
    /// manifest is found.
    #[tracing::instrument]
    pub fn parse_manifest(&self) -> Result<Option<PackageManifest>, ManifestError> {
        let manifest = fs::read_to_string(self.manifest_file())
            .ignore(io_not_found)?
            .as_deref()
            .map(toml::from_str)
            .transpose()?;

        if let Some(manifest) = &manifest {
            validate_manifest(manifest)?;
        }

        Ok(manifest)
    }

    /// Parses the manifest config from the tool section. Returns `None` if no
    /// tool section found.
    #[tracing::instrument]
    pub fn parse_config(
        &self,
        manifest: &PackageManifest,
    ) -> Result<Option<ProjectConfig>, ManifestError> {
        let config = manifest
            .tool
            .sections
            .get(TOOL_NAME)
            .cloned()
            .map(ProjectConfig::deserialize)
            .transpose()?;

        if let Some(config) = &config {
            validate_config(config)?;
        }

        Ok(config)
    }

    /// Reads the project's unit test template if it exists. Returns `None` if
    /// no template was found.
    #[tracing::instrument]
    pub fn read_unit_test_template(
        &self,
        config: &ProjectConfig,
    ) -> Result<Option<String>, io::Error> {
        let root = Path::new(&config.unit_tests_root);
        let template = root.join("template.typ");

        fs::read_to_string(template).ignore(io_not_found)
    }
}

impl ShallowProject {
    /// Returns the path to the project root.
    ///
    /// The project root is used to resolve absolute paths in typst when
    /// executing tests.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the path to the project manifest (`typst.toml`).
    pub fn manifest_file(&self) -> PathBuf {
        self.root.join(MANIFEST_FILE)
    }

    /// Returns the path to the vcs root.
    ///
    /// The vcs root is used for properly handling non-persistent storage of
    /// tests.
    pub fn vcs_root(&self) -> Option<&Path> {
        self.vcs.as_ref().map(Vcs::root)
    }
}

/// A fully loaded project, this can be constructed from [`ShallowProject`],
/// which can be used to discover project paths without loading any
/// configuration or manifests.
#[derive(Debug, Clone)]
pub struct Project {
    base: ShallowProject,
    manifest: Option<PackageManifest>,
    config: ProjectConfig,
    unit_test_template: Option<String>,
}

impl Project {
    /// Create a new empty project.
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Self {
            base: ShallowProject {
                root: root.into(),
                vcs: None,
            },
            manifest: None,
            config: ProjectConfig::default(),
            unit_test_template: None,
        }
    }

    /// Attach a version control system to this project.
    pub fn with_vcs(mut self, vcs: Option<Vcs>) -> Self {
        self.base.vcs = vcs;
        self
    }

    /// Attach a parsed manifest to this project.
    pub fn with_manifest(mut self, manifest: Option<PackageManifest>) -> Self {
        self.manifest = manifest;
        self
    }

    /// Attach a parsed project config to this project.
    pub fn with_config(mut self, config: ProjectConfig) -> Self {
        self.config = config;
        self
    }

    /// Attach a unit test template to this project.
    pub fn with_unit_test_template(mut self, unit_test_template: Option<String>) -> Self {
        self.unit_test_template = unit_test_template;
        self
    }

    /// Checks the given directory for a project root, returning `true` if it
    /// was found.
    pub fn exists_at(dir: &Path) -> io::Result<bool> {
        if dir.join(MANIFEST_FILE).try_exists()? {
            return Ok(true);
        }

        Ok(false)
    }
}

impl Project {
    /// Returns the shallow base object for this project.
    pub fn base(&self) -> &ShallowProject {
        &self.base
    }

    /// The fully parsed project manifest.
    pub fn manifest(&self) -> Option<&PackageManifest> {
        self.manifest.as_ref()
    }

    /// A package spec for this package itself, this is used by template tests
    /// refer to themselves without attempting to download the package.
    pub fn package_spec(&self) -> Option<PackageSpec> {
        self.manifest.as_ref().map(|m| PackageSpec {
            namespace: "preview".into(),
            name: m.package.name.clone(),
            version: m.package.version,
        })
    }

    /// The fully parsed project config layer.
    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    /// Returns the unit test template, that is, the source template to
    /// use when generating new unit tests.
    pub fn unit_test_template(&self) -> Option<&str> {
        self.unit_test_template.as_deref()
    }

    /// Returns the [`Vcs`] this project is managed by or `None` if no supported
    /// Vcs was found.
    pub fn vcs(&self) -> Option<&Vcs> {
        self.base.vcs.as_ref()
    }
}

impl Project {
    /// Returns the path to the test root. That is the path within the project
    /// root where the test suite is located.
    ///
    /// The test root is used to resolve test identifiers.
    pub fn unit_tests_root(&self) -> PathBuf {
        self.root().join(&self.config.unit_tests_root)
    }

    /// Returns the root path of the template directory.
    pub fn template_root(&self) -> Option<PathBuf> {
        self.manifest
            .as_ref()
            .and_then(|m| m.template.as_ref())
            .map(|t| self.root().join(t.path.as_str()))
    }

    /// Returns the entrypoint script inside the template directory.
    pub fn template_entrypoint(&self) -> Option<PathBuf> {
        self.manifest
            .as_ref()
            .and_then(|m| m.template.as_ref())
            .map(|t| {
                let mut root = self.root().to_path_buf();
                root.push(t.path.as_str());
                root.push(t.entrypoint.as_str());
                root
            })
    }

    /// Returns the path to the unit test template, that is, the source template to
    /// use when generating new unit tests.
    pub fn unit_test_template_file(&self) -> PathBuf {
        let mut dir = self.unit_tests_root();
        dir.push("template.typ");
        dir
    }

    /// Create a path to the test directory for the given identifier.
    pub fn unit_test_dir(&self, id: &Id) -> PathBuf {
        let mut dir = self.unit_tests_root();
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

impl Deref for Project {
    type Target = ShallowProject;

    fn deref(&self) -> &Self::Target {
        self.base()
    }
}

fn validate_manifest(manifest: &PackageManifest) -> Result<(), ValidationError> {
    let PackageManifest {
        package: _,
        template,
        tool: _,
        unknown_fields: _,
    } = manifest;

    let Some(template) = template else {
        return Ok(());
    };

    let mut error = ValidationError {
        errors: BTreeMap::new(),
    };

    if !is_trivial_path(template.path.as_str()) {
        error
            .errors
            .insert("template.path".into(), ValidationErrorCause::NonTrivialPath);
    }

    if !is_trivial_path(template.entrypoint.as_str()) {
        error.errors.insert(
            "template.entrypoint".into(),
            ValidationErrorCause::NonTrivialPath,
        );
    }

    if !error.errors.is_empty() {
        return Err(error);
    }

    Ok(())
}

fn validate_config(config: &ProjectConfig) -> Result<(), ValidationError> {
    let ProjectConfig {
        unit_tests_root,
        defaults: _,
    } = config;

    let mut error = ValidationError {
        errors: BTreeMap::new(),
    };

    if !is_trivial_path(unit_tests_root.as_str()) {
        error
            .errors
            .insert("tests".into(), ValidationErrorCause::NonTrivialPath);
    }

    if !error.errors.is_empty() {
        return Err(error);
    }

    Ok(())
}

fn is_trivial_path<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref();
    path.is_relative() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Returned by [`ShallowProject::load`].
#[derive(Debug, Error)]
pub enum LoadError {
    /// An error occurred while parsing the project manifest.
    #[error("an error occurred while parsing the project manifest")]
    Manifest(#[from] ManifestError),

    /// An error occurred while parsing the project config.
    #[error("an error occurred while parsing the project config")]
    Config(#[from] ConfigError),

    /// An io error occurred.
    #[error("an io error occurred")]
    Io(#[from] io::Error),
}

/// Contained in [`ConfigError`] and [`ManifestError`].
#[derive(Debug, Error)]
#[error("encountered {} errors while validating", errors.len())]
pub struct ValidationError {
    /// The inner errors for each field.
    pub errors: BTreeMap<EcoString, ValidationErrorCause>,
}

/// The cause for a validation error of an individual field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationErrorCause {
    /// A path was not trivial when it must be, i.e. it contained components
    /// such as `.` or `..`.
    NonTrivialPath,
}

/// Returned by [`ShallowProject::parse_config`].
#[derive(Debug, Error)]
pub enum ConfigError {
    /// An error occurred while validating the project config.
    #[error("an error occurred while validating project config")]
    Invalid(#[from] ValidationError),

    /// An error occurred while parsing the project config.
    #[error("an error occurred while parsing the project config")]
    Parse(#[from] toml::de::Error),

    /// An io error occurred.
    #[error("an io error occurred")]
    Io(#[from] io::Error),
}

/// Returned by [`ShallowProject::parse_manifest`].
#[derive(Debug, Error)]
pub enum ManifestError {
    /// An error occurred while validating the project manifest.
    #[error("an error occurred while validating project manifest")]
    Invalid(#[from] ValidationError),

    /// An error occurred while parsing the project manifest.
    #[error("an error occurred while parsing the project manifest")]
    Parse(#[from] toml::de::Error),

    /// An io error occurred.
    #[error("an io error occurred")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use tytanic_utils::typst::{PackageManifestBuilder, TemplateInfoBuilder};

    use super::*;

    #[test]
    fn test_template_paths() {
        let project = Project::new("root").with_manifest(Some(
            PackageManifestBuilder::new()
                .template(
                    TemplateInfoBuilder::new()
                        .path("foo")
                        .entrypoint("bar.typ")
                        .build(),
                )
                .build(),
        ));

        assert_eq!(
            project.template_root(),
            Some(PathBuf::from_iter(["root", "foo"]))
        );
        assert_eq!(
            project.template_entrypoint(),
            Some(PathBuf::from_iter(["root", "foo", "bar.typ"]))
        );
    }

    #[test]
    fn test_unit_test_paths() {
        let project = Project::new("root");
        let id = Id::new("a/b").unwrap();

        assert_eq!(
            project.unit_tests_root(),
            PathBuf::from_iter(["root", "tests"])
        );
        assert_eq!(
            project.unit_test_dir(&id),
            PathBuf::from_iter(["root", "tests", "a", "b"])
        );
        assert_eq!(
            project.unit_test_script(&id),
            PathBuf::from_iter(["root", "tests", "a", "b", "test.typ"])
        );

        let project = Project::new("root").with_config(ProjectConfig {
            unit_tests_root: "foo".into(),
            ..Default::default()
        });

        assert_eq!(
            project.unit_test_ref_script(&id),
            PathBuf::from_iter(["root", "foo", "a", "b", "ref.typ"])
        );
        assert_eq!(
            project.unit_test_ref_dir(&id),
            PathBuf::from_iter(["root", "foo", "a", "b", "ref"])
        );
        assert_eq!(
            project.unit_test_out_dir(&id),
            PathBuf::from_iter(["root", "foo", "a", "b", "out"])
        );
        assert_eq!(
            project.unit_test_diff_dir(&id),
            PathBuf::from_iter(["root", "foo", "a", "b", "diff"])
        );
    }

    #[test]
    fn test_validation_default() {
        let config = ProjectConfig::default();
        validate_config(&config).unwrap();
    }

    #[test]
    fn test_validation_trivial_paths() {
        let manifest = PackageManifestBuilder::new()
            .template(
                TemplateInfoBuilder::new()
                    .path("foo")
                    .entrypoint("bar.typ")
                    .build(),
            )
            .build();

        let config = ProjectConfig {
            unit_tests_root: "qux".into(),
            ..Default::default()
        };

        validate_manifest(&manifest).unwrap();
        validate_config(&config).unwrap();
    }

    #[test]
    fn test_validation_non_trival_paths() {
        let manifest = PackageManifestBuilder::new()
            .template(TemplateInfoBuilder::new().path("..").build())
            .build();

        let config = ProjectConfig {
            unit_tests_root: "/.".into(),
            ..Default::default()
        };

        let manifest = validate_manifest(&manifest).unwrap_err();
        let config = validate_config(&config).unwrap_err();

        assert_eq!(manifest.errors.len(), 1);
        assert_eq!(config.errors.len(), 1);

        assert_eq!(
            manifest.errors.get("template.path").unwrap(),
            &ValidationErrorCause::NonTrivialPath
        );
        assert_eq!(
            config.errors.get("tests").unwrap(),
            &ValidationErrorCause::NonTrivialPath
        );
    }
}
