//! Library integration tests.

use bivvy::BivvyError;

#[test]
fn error_types_are_public() {
    let err = BivvyError::UnknownTemplate {
        name: "test".into(),
    };
    assert!(err.to_string().contains("test"));
}

#[test]
fn result_type_alias_is_public() {
    fn test_fn() -> bivvy::Result<()> {
        Ok(())
    }
    assert!(test_fn().is_ok());
}

#[test]
fn cli_types_are_public() {
    use bivvy::cli::{Cli, Commands};
    use clap::Parser;

    // Actually test parsing with parse_from
    let cli = Cli::parse_from(["bivvy", "status", "--json"]);
    assert!(cli.command.is_some());

    if let Some(Commands::Status(args)) = cli.command {
        assert!(args.json);
    } else {
        panic!("Expected Status command");
    }
}
