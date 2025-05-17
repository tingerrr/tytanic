use std::io::Write;
use std::ops::Not;

use color_eyre::eyre;
use termcolor::Color;
use typst::diag::Warned;
use typst_syntax::FileId;
use typst_syntax::Source;
use typst_syntax::VirtualPath;
use tytanic_core::doc::render::ppi_to_ppp;
use tytanic_core::doc::Document;
use tytanic_core::test::unit::Kind;
use tytanic_core::test::unit::Reference;
use tytanic_core::test::unit::DEFAULT_TEST_INPUT;
use tytanic_core::test::Id;
use tytanic_core::test::UnitTest;

use super::CompileOptions;
use super::Context;
use super::ExportOptions;
use super::KindOption;
use super::OptionDelegate;
use super::Switch;
use super::TemplateSwitch;
use crate::cli::OperationFailure;
use crate::cwriteln;
use crate::ui;
use crate::DEFAULT_OPTIMIZE_OPTIONS;

#[derive(clap::Args, Debug, Clone)]
#[group(id = "new-args")]
pub struct Args {
    /// The type of test to create.
    #[arg(long = "type", short, group = "type", default_value = "persistent")]
    pub kind: KindOption,

    /// Shorthand for `--type=persistent`.
    #[arg(long, short = 'P', group = "type")]
    pub persistent: bool,

    /// Shorthand for `--type=ephermeral`.
    #[arg(long, short = 'E', group = "type")]
    pub ephemeral: bool,

    /// Shorthand for `--type=compile-only`.
    #[arg(long, short = 'C', group = "type")]
    pub compile_only: bool,

    #[command(flatten)]
    pub template: TemplateSwitch,

    #[command(flatten)]
    pub compile: CompileOptions,

    #[command(flatten)]
    pub export: ExportOptions,

    /// The name of the new test.
    #[arg(value_name = "NAME")]
    pub test: Id,
}

pub fn run(ctx: &mut Context, args: &Args) -> eyre::Result<()> {
    if args.test == Id::template() {
        writeln!(ctx.ui.error()?, "Cannot create template test")?;
        eyre::bail!(OperationFailure);
    }

    let project = ctx.project()?;
    let suite = ctx.collect_tests(&project)?;

    if suite.contains(&args.test) {
        let mut w = ctx.ui.error()?;

        write!(w, "Test ")?;
        ui::write_test_id(&mut w, &args.test)?;
        writeln!(w, " already exists")?;
        eyre::bail!(OperationFailure);
    }

    let vcs = project.vcs();
    let id = args.test.clone();

    let kind = if args.persistent {
        Kind::Persistent
    } else if args.ephemeral {
        Kind::Ephemeral
    } else if args.compile_only {
        Kind::CompileOnly
    } else {
        args.kind.into_native()
    };

    let source = project
        .unit_test_template()
        .filter(|_| args.template.get_or_default())
        .unwrap_or(DEFAULT_TEST_INPUT);

    let reference = match kind {
        Kind::CompileOnly => None,
        Kind::Ephemeral => Some(Reference::Ephemeral(source.into())),
        Kind::Persistent => {
            let providers =
                ctx.providers(&project, &ctx.args.package, &ctx.args.font, &args.compile)?;

            let path = project.unit_test_template_file();

            let path = path
                .strip_prefix(project.root())
                .expect("template is in project root");

            let id = FileId::new(None, VirtualPath::new(path));
            let world = providers.system_world(id);

            let Warned { output, warnings } = Document::compile(
                Source::new(id, source.into()),
                &world,
                ppi_to_ppp(args.export.ppi.unwrap_or(project.config().defaults.ppi)),
                args.compile.warnings.into_native(),
            );

            let doc = match output {
                Ok(doc) => {
                    ui::write_diagnostics(
                        &mut ctx.ui.stderr(),
                        ctx.ui.diagnostic_config(),
                        &world,
                        &warnings,
                        &[],
                    )?;
                    doc
                }
                Err(err) => {
                    ui::write_diagnostics(
                        &mut ctx.ui.stderr(),
                        ctx.ui.diagnostic_config(),
                        &world,
                        &warnings,
                        &err.0,
                    )?;
                    eyre::bail!(OperationFailure);
                }
            };

            Some(Reference::Persistent {
                doc,
                opt: args
                    .export
                    .optimize_refs
                    .get_or_default()
                    .not()
                    .then(|| Box::new(DEFAULT_OPTIMIZE_OPTIONS.clone())),
            })
        }
    };

    UnitTest::create(&project, vcs, id, source, reference)?;

    let mut w = ctx.ui.stderr();

    write!(w, "Added ")?;
    cwriteln!(colored(w, Color::Cyan), "{}", args.test)?;

    Ok(())
}
