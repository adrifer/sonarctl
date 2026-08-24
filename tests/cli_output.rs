//! CLI argument handling and rendered output.

mod common;

use clap::Parser;
use sonarctl::app::VolumeChange;
use sonarctl::cli::{Cli, Command, parse_channels, parse_mixer_channels, parse_volume_change};
use sonarctl::output;
use sonarctl::sonar::models::{Channel, MixerChannel};

use common::{fixture_applications, fixture_devices, fixture_routes, fixture_volumes};

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
fn parses_mixer_commands_and_values() {
    assert_eq!(
        parse_mixer_channels("master,chat,mic").unwrap(),
        vec![
            MixerChannel::Master,
            MixerChannel::Chat,
            MixerChannel::Microphone
        ]
    );
    assert_eq!(
        parse_mixer_channels("all").unwrap(),
        MixerChannel::ALL.to_vec()
    );
    assert_eq!(
        parse_volume_change("75%").unwrap(),
        VolumeChange::Absolute(75.0)
    );
    assert_eq!(
        parse_volume_change("+5").unwrap(),
        VolumeChange::Relative(5.0)
    );
    assert_eq!(
        parse_volume_change("-5").unwrap(),
        VolumeChange::Relative(-5.0)
    );
    assert!(parse_volume_change("loud").is_err());

    assert!(matches!(
        parse(&["sonarctl", "volume", "game", "-5"]).command,
        Some(Command::Volume(_))
    ));
    assert!(matches!(
        parse(&["sonarctl", "mute", "chat", "toggle"]).command,
        Some(Command::Mute(_))
    ));
    assert!(matches!(
        parse(&["sonarctl", "unmute", "all", "--json"]).command,
        Some(Command::Unmute(_))
    ));
}

#[test]
fn parses_application_commands() {
    assert!(matches!(
        parse(&["sonarctl", "apps", "--json"]).command,
        Some(Command::Apps { json: true })
    ));
    let cli = parse(&["sonarctl", "app", "set", "Discord.exe", "chat"]);
    match cli.command {
        Some(Command::App {
            command: sonarctl::cli::AppCommand::Set(args),
        }) => {
            assert_eq!(args.selector, "Discord.exe");
            assert_eq!(args.channel.as_deref(), Some("chat"));
            assert_eq!(args.pid, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
    let cli = parse(&["sonarctl", "app", "set", "--pid", "200", "media"]);
    match cli.command {
        Some(Command::App {
            command: sonarctl::cli::AppCommand::Set(args),
        }) => {
            assert_eq!(args.selector, "media");
            assert!(args.channel.is_none());
            assert_eq!(args.pid, Some(200));
        }
        other => panic!("unexpected command: {other:?}"),
    }
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

    let volumes = output::volumes_json(&fixture_volumes());
    assert_eq!(volumes["channels"][0]["channel"], "master");
    assert_eq!(volumes["channels"][0]["volume"], 80.0);
    assert_eq!(volumes["channels"][2]["muted"], true);
}

#[test]
fn volume_output_is_a_plain_table() {
    let text = output::volumes_text(&fixture_volumes());
    assert!(text.starts_with("CHANNEL     VOLUME  STATE\n"));
    assert!(text.contains("Master      80%     unmuted"));
    assert!(text.contains("Chat        75%     muted"));
}

#[test]
fn application_output_has_stable_text_and_json() {
    let applications = fixture_applications();
    let text = output::applications_text(&applications);
    assert!(text.starts_with("APPLICATION     PID  CHANNEL"));
    assert!(text.contains("Microsoft Edge  200  Media"));
    assert!(text.contains("Windows App     300  Chat"));

    let value = output::applications_json(&applications);
    assert_eq!(value["applications"].as_array().unwrap().len(), 5);
    assert_eq!(value["applications"][0]["process_id"], 200);
    assert_eq!(value["applications"][0]["channel"], "media");
    assert_eq!(value["applications"][0]["activity"], "active");
}
