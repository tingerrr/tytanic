//! Common report PODs for stable JSON representation of internal entities.

use serde::Serialize;
use typst_syntax::package::{PackageManifest, PackageVersion};
use tytanic_core::project::Project;
use tytanic_core::suite::Suite;
use tytanic_core::test::Test;
use tytanic_core::{TemplateTest, UnitTest};

#[derive(Debug, Serialize)]
pub struct ProjectJson<'m, 's> {
    pub package: Option<PackageJson<'m>>,
    pub vcs: Option<String>,
    pub tests: Vec<UnitTestJson<'s>>,
    pub is_template: bool,
}

impl<'m, 's> ProjectJson<'m, 's> {
    pub fn new(project: &Project, manifest: Option<&'m PackageManifest>, suite: &'s Suite) -> Self {
        Self {
            package: manifest.map(|m| PackageJson {
                name: &m.package.name,
                version: &m.package.version,
            }),
            vcs: project.vcs().map(|vcs| vcs.to_string()),
            tests: suite.unit_tests().map(UnitTestJson::new).collect(),
            is_template: manifest.and_then(|m| m.template.as_ref()).is_some(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PackageJson<'p> {
    pub name: &'p str,
    pub version: &'p PackageVersion,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "test")]
pub enum TestJson<'t> {
    #[serde(rename = "unit")]
    Unit(UnitTestJson<'t>),

    #[serde(rename = "template")]
    Template(TemplateTestJson<'t>),
}

impl<'t> TestJson<'t> {
    pub fn new(test: &'t Test) -> Self {
        match test {
            Test::Unit(test) => Self::Unit(UnitTestJson::new(test)),
            Test::Template(test) => Self::Template(TemplateTestJson::new(test)),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UnitTestJson<'t> {
    pub id: &'t str,
    pub kind: &'static str,
    pub is_skip: bool,
}

impl<'t> UnitTestJson<'t> {
    pub fn new(test: &'t UnitTest) -> Self {
        Self {
            id: test.id().as_str(),
            kind: test.kind().as_str(),
            is_skip: test.is_skip(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TemplateTestJson<'t> {
    pub id: &'t str,
}

impl<'t> TemplateTestJson<'t> {
    pub fn new(test: &'t TemplateTest) -> Self {
        Self {
            id: test.id().as_str(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FontVariantJson {
    pub weight: u16,
    pub style: &'static str,
    pub stretch: f64,
}

#[derive(Debug, Serialize)]
pub struct FontJson<'f> {
    pub name: &'f str,
    pub variants: Vec<FontVariantJson>,
}

#[derive(Serialize)]
pub struct FailedJson {
    pub compilation: usize,
    pub comparison: usize,
    pub otherwise: usize,
}

#[derive(Serialize)]
pub struct DurationJson {
    pub seconds: u64,
    pub nanoseconds: u32,
}
