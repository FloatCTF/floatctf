//! CLI argument parsing tests.

use clap::Parser;
use fcmc::{Args, Commands, GenFormat};

#[test]
fn cli_check_default_path() {
    let args = Args::try_parse_from(["fcmc", "check"]).unwrap();
    match args.command {
        Commands::Check { path, .. } => {
            assert!(path.is_none());
        }
        _ => panic!("expected Check command"),
    }
}

#[test]
fn cli_check_with_path() {
    let args = Args::try_parse_from(["fcmc", "check", "-p", "/some/path"]).unwrap();
    match args.command {
        Commands::Check { path, .. } => {
            assert_eq!(path.as_deref(), Some("/some/path"));
        }
        _ => panic!("expected Check command"),
    }
}

#[test]
fn cli_build_default() {
    let args = Args::try_parse_from(["fcmc", "build"]).unwrap();
    match args.command {
        Commands::Build { path, format } => {
            assert!(path.is_none());
            assert_eq!(format, GenFormat::Challenge);
        }
        _ => panic!("expected Build command"),
    }
}

#[test]
fn cli_build_with_format() {
    let args = Args::try_parse_from(["fcmc", "build", "-f", "gamebox"]).unwrap();
    match args.command {
        Commands::Build { format, .. } => {
            assert_eq!(format, GenFormat::Gamebox);
        }
        _ => panic!("expected Build command"),
    }
}

#[test]
fn cli_gen_required_name() {
    let result = Args::try_parse_from(["fcmc", "gen"]);
    assert!(result.is_err());
}

#[test]
fn cli_gen_with_name() {
    let args = Args::try_parse_from(["fcmc", "gen", "-n", "my-challenge"]).unwrap();
    match args.command {
        Commands::Gen {
            name,
            output,
            format,
            template,
        } => {
            assert_eq!(name, "my-challenge");
            assert_eq!(output, ".");
            assert_eq!(format, GenFormat::Challenge);
            assert!(!template);
        }
        _ => panic!("expected Gen command"),
    }
}

#[test]
fn cli_gen_gamebox_with_template() {
    let args = Args::try_parse_from(["fcmc", "gen", "-n", "box", "-f", "gamebox", "-t"]).unwrap();
    match args.command {
        Commands::Gen {
            format, template, ..
        } => {
            assert_eq!(format, GenFormat::Gamebox);
            assert!(template);
        }
        _ => panic!("expected Gen command"),
    }
}

#[test]
fn cli_gen_format_aliases() {
    // "c" is alias for Challenge
    let args = Args::try_parse_from(["fcmc", "gen", "-n", "x", "-f", "c"]).unwrap();
    match args.command {
        Commands::Gen { format, .. } => assert_eq!(format, GenFormat::Challenge),
        _ => panic!("expected Gen command"),
    }

    // "g" is alias for Gamebox
    let args = Args::try_parse_from(["fcmc", "gen", "-n", "x", "-f", "g"]).unwrap();
    match args.command {
        Commands::Gen { format, .. } => assert_eq!(format, GenFormat::Gamebox),
        _ => panic!("expected Gen command"),
    }
}

#[test]
fn cli_no_subcommand_fails() {
    let result = Args::try_parse_from(["fcmc"]);
    assert!(result.is_err());
}
