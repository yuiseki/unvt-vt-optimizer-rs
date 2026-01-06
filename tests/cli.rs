use clap::Parser;

use tile_prune::cli::{Cli, Command, StyleMode};

#[test]
fn parse_optimize_minimal() {
    let cli = Cli::parse_from(["tile-prune", "optimize", "hoge.mbtiles"]);
    match cli.command {
        Command::Optimize(args) => {
            assert_eq!(args.input.as_os_str(), "hoge.mbtiles");
            assert_eq!(args.output, None);
            assert_eq!(args.input_format, None);
            assert_eq!(args.output_format, None);
            assert_eq!(args.style, None);
            assert_eq!(args.style_mode, StyleMode::LayerFilter);
            assert_eq!(args.max_tile_bytes, 1_280_000);
            assert_eq!(args.threads, None);
            assert_eq!(args.io_batch, 1_000);
            assert_eq!(args.checkpoint, None);
            assert!(!args.resume);
        }
        _ => panic!("expected optimize command"),
    }
}

#[test]
fn parse_optimize_options() {
    let cli = Cli::parse_from([
        "tile-prune",
        "optimize",
        "planet.mbtiles",
        "--output",
        "out.pmtiles",
        "--input-format",
        "mbtiles",
        "--output-format",
        "pmtiles",
        "--style",
        "style.json",
        "--style-mode",
        "layer+filter",
        "--max-tile-bytes",
        "2048",
        "--threads",
        "8",
        "--io-batch",
        "200",
        "--checkpoint",
        "state.json",
        "--resume",
    ]);

    match cli.command {
        Command::Optimize(args) => {
            assert_eq!(args.input.as_os_str(), "planet.mbtiles");
            assert_eq!(args.output.unwrap().as_os_str(), "out.pmtiles");
            assert_eq!(args.input_format.unwrap(), "mbtiles");
            assert_eq!(args.output_format.unwrap(), "pmtiles");
            assert_eq!(args.style.unwrap().as_os_str(), "style.json");
            assert_eq!(args.style_mode, StyleMode::LayerFilter);
            assert_eq!(args.max_tile_bytes, 2048);
            assert_eq!(args.threads, Some(8));
            assert_eq!(args.io_batch, 200);
            assert_eq!(args.checkpoint.unwrap().as_os_str(), "state.json");
            assert!(args.resume);
        }
        _ => panic!("expected optimize command"),
    }
}

#[test]
fn parse_optimize_style_modes() {
    let cli = Cli::parse_from(["tile-prune", "optimize", "in.mbtiles", "--style-mode", "none"]);
    match cli.command {
        Command::Optimize(args) => {
            assert_eq!(args.style_mode, StyleMode::None);
        }
        _ => panic!("expected optimize command"),
    }

    let cli = Cli::parse_from(["tile-prune", "optimize", "in.mbtiles", "--style-mode", "layer"]);
    match cli.command {
        Command::Optimize(args) => {
            assert_eq!(args.style_mode, StyleMode::Layer);
        }
        _ => panic!("expected optimize command"),
    }
}
