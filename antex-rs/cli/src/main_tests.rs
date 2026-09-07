use clap::Parser;

use super::Action;
use super::Args;

#[test]
fn no_subcommand_selects_the_direct_tui() {
    let args = Args::try_parse_from(["antex"]).unwrap();
    assert!(args.command.is_none());
    assert!(matches!(args.command.unwrap_or(Action::Tui), Action::Tui));
}
