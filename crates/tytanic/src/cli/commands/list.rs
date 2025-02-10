use std::io::Write;

use color_eyre::eyre;
use termcolor::Color;
use tytanic_core::test::Kind as TestKind;

use super::{Context, FilterOptions};
use crate::json::TestJson;
use crate::{cwriteln, ui};

#[derive(clap::Args, Debug, Clone)]
#[group(id = "list-args")]
pub struct Args {
    /// Print a JSON describing the project to stdout
    #[arg(long)]
    pub json: bool,

    #[command(flatten)]
    pub filter: FilterOptions,
}

pub fn run(ctx: &mut Context, args: &Args) -> eyre::Result<()> {
    let project = ctx.project()?;
    let suite = ctx.collect_tests_with_filter(&project, ctx.filter(&args.filter)?)?;

    if args.json {
        serde_json::to_writer_pretty(
            ctx.ui.stdout(),
            &suite
                .matched()
                .values()
                .map(TestJson::new)
                .collect::<Vec<_>>(),
        )?;

        return Ok(());
    }

    let mut w = ctx.ui.stderr();

    // NOTE(tinger): max padding of 50 should be enough for most cases
    let pad = Ord::min(
        suite
            .matched()
            .keys()
            .map(|id| id.len())
            .max()
            .unwrap_or(usize::MAX),
        50,
    );

    for (id, test) in suite.matched() {
        ui::write_test_id(&mut w, id)?;
        if let Some(pad) = pad.checked_sub(id.len()) {
            write!(w, "{: >pad$} ", "")?;
        }
        let color = match test.kind() {
            TestKind::Ephemeral => Color::Yellow,
            TestKind::Persistent => Color::Green,
            TestKind::CompileOnly => Color::Yellow,
        };
        cwriteln!(bold_colored(w, color), "{}", test.kind().as_str())?;
    }

    Ok(())
}
