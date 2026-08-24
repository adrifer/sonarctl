//! CLI argument handling and rendered output.

mod common;

use clap::Parser;
use sonarctl::cli::{Cli, Command, parse_channels};
use sonarctl::output;
use sonarctl::sonar::models::Channel;

use common::{fixture_devices, fixture_routes};

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(args).expect("valid arguments")
}

#[test]
fn no_subcommand_opens_the_tui() {
    assert!(parse(&["sonarctl"]).command.is_none());
    assert!(matches!(
        parse(&["sonarctl", "tui"]).command,
        Some(Command::Tui)
    ));
}

#[test]
fn global_flags_are_accepted_anywhere() {
    let cli = parse(&["sonarctl", "-vv", "status", "--json"]);
    assert_eq!(cli.verbose, 2);
    assert!(matches!(cli.command, Some(Command::Status { json: true })));

    let cli = parse(&["sonarctl", "doctor", "--core-props", "C:/x/coreProps.json"]);
    assert_eq!(
        cli.core_props.unwrap().to_string_lossy(),
        "C:/x/coreProps.json"
    );
}

#[test]
fn device_filters_are_mutually_exclusive() {
    let cli = parse(&["sonarctl", "devices", "--playback"]);
    match cli.command {
        Some(Command::Devices(args)) => {
            assert_eq!(
                args.role(),
                Some(sonarctl::sonar::models::DeviceRole::Playback)
            );
        }
        other => panic!("unexpected command: {other:?}"),
    }

    assert!(Cli::try_parse_from(["sonarctl", "devices", "--playback", "--capture"]).is_err());
}

#[test]
fn set_requires_a_device_or_an_id() {
    assert!(Cli::try_parse_from(["sonarctl", "set", "game"]).is_err());
    assert!(Cli::try_parse_from(["sonarctl", "set", "game", "tv", "--id", "x"]).is_err());

    let cli = parse(&["sonarctl", "set", "game", "--id", "{abc}"]);
    match cli.command {
        Some(Command::Set(args)) => {
            assert_eq!(args.id.as_deref(), Some("{abc}"));
            assert!(args.device.is_none());
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_channel_lists() {
    assert_eq!(parse_channels("game").unwrap(), vec![Channel::Game]);
    assert_eq!(
        parse_channels("game, chat ,media").unwrap(),
        vec![Channel::Game, Channel::Chat, Channel::Media]
    );
    assert_eq!(
        parse_channels("gaming,game").unwrap(),
        vec![Channel::Game],
        "duplicates collapse"
    );

    let err = parse_channels("game,speakers").unwrap_err();
    assert_eq!(err.exit_code(), 2);
    assert!(parse_channels(" , ").is_err());
}

#[test]
fn status_output_is_a_plain_table() {
    let text = output::status_text(&fixture_routes());
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines[0], "Sonar: running");
    assert_eq!(lines[2], "CHANNEL     DEVICE");
    assert_eq!(lines[3], "Game        Arctis Nova Pro Wireless");
    assert_eq!(lines[7], "Microphone  Shure MV7");
    assert!(!text.contains('\u{1b}'), "no ANSI escapes");
}

#[test]
fn devices_output_lists_type_and_name() {
    let devices: Vec<_> = fixture_devices()
        .into_iter()
        .filter(|device| device.is_physical())
        .collect();
    let text = output::devices_text(&devices);

    assert!(text.starts_with("TYPE      NAME\n"));
    assert!(text.contains("Playback  Arctis Nova Pro Wireless"));
    assert!(text.contains("Capture   Shure MV7"));
    assert!(!text.contains("SteelSeries Sonar -"));
}

#[test]
fn get_output_is_script_friendly() {
    let routes = fixture_routes();
    let game = routes.iter().find(|r| r.channel == Channel::Game).unwrap();
    assert_eq!(output::get_text(game), "Arctis Nova Pro Wireless\n");
}

#[test]
fn set_output_avoids_unicode_when_not_a_tty() {
    let device = fixture_devices()
        .into_iter()
        .find(|device| device.name == "LG TV")
        .unwrap();
    assert_eq!(
        output::set_text(Channel::Media, &device, false),
        "Media -> LG TV\n"
    );
    assert_eq!(
        output::set_text(Channel::Media, &device, true),
        "Media → LG TV\n"
    );
}

#[test]
fn json_output_has_a_stable_schema() {
    let routes = fixture_routes();
    let game = routes.iter().find(|r| r.channel == Channel::Game).unwrap();
    let value = output::get_json(game);

    assert_eq!(value["channel"], "game");
    assert_eq!(value["device"]["name"], "Arctis Nova Pro Wireless");
    assert_eq!(value["device"]["id"], game.device_id.as_str());

    let status = output::status_json(&routes);
    assert_eq!(status["channels"].as_array().unwrap().len(), 5);
    assert_eq!(status["channels"][4]["channel"], "microphone");

    let devices = output::devices_json(&fixture_devices());
    assert_eq!(devices["devices"][0]["role"], "playback");
    assert_eq!(devices["devices"][0]["enabled"], true);

    let rendered = output::json_line(&value);
    assert!(rendered.ends_with('\n'));
    serde_json::from_str::<serde_json::Value>(&rendered).expect("valid JSON");
}
