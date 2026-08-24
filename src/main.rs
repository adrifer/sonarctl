//! sonarctl entry point.

use std::io::{IsTerminal, Write};
use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use sonarctl::app::{App, ApplicationSelector, DeviceSelector, MuteChange};
use sonarctl::cli::{
    AppCommand, AppSetArgs, Cli, Command, ConfigCommand, DevicesArgs, MixerChannelsArgs,
    MuteAction, MuteArgs, SetArgs, VolumeArgs, parse_channels, parse_mixer_channels,
    parse_volume_change,
};
use sonarctl::config::Config;
use sonarctl::doctor;
use sonarctl::error::{Error, Result, exit_code};
use sonarctl::output;
use sonarctl::sonar::backend::SonarHttpBackend;
use sonarctl::sonar::discovery::DiscoveryOptions;
use sonarctl::sonar::models::Channel;
use sonarctl::tui;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    let verbose = cli.verbose;
    match run(cli).await {
        Ok(()) => std::process::exit(exit_code::SUCCESS),
        Err(err) => {
            report(&err, verbose);
            std::process::exit(err.exit_code());
        }
    }
}

fn init_tracing(verbose: u8) {
    let filter = match std::env::var("RUST_LOG") {
        Ok(value) if !value.is_empty() => EnvFilter::new(value),
        _ => EnvFilter::new(match verbose {
            0 => "warn",
            1 => "warn,sonarctl=debug",
            _ => "warn,sonarctl=trace,reqwest=debug",
        }),
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .without_time()
        .try_init();
}

fn report(err: &Error, verbose: u8) {
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "error: {err}");
    if let Some(hint) = err.hint() {
        let _ = writeln!(stderr);
        let _ = writeln!(stderr, "{hint}");
    }
    if verbose > 0
        && let Some(detail) = err.detail()
    {
        let _ = writeln!(stderr);
        let _ = writeln!(stderr, "details: {detail}");
    }
}

fn print(text: &str) -> Result<()> {
    let mut stdout = std::io::stdout();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|err| Error::Other(format!("could not write output: {err}")))
}

fn build_app(cli: &Cli) -> Result<App> {
    let options = DiscoveryOptions::resolve(cli.core_props.clone());
    let config = Config::load()?;
    let backend = Arc::new(SonarHttpBackend::new(options));
    Ok(App::new(backend, config))
}

async fn run(cli: Cli) -> Result<()> {
    match &cli.command {
        None | Some(Command::Tui) => {
            let app = build_app(&cli)?;
            tui::run(app).await
        }
        Some(Command::Doctor) => run_doctor(&cli).await,
        Some(Command::Status { json }) => run_status(&cli, *json).await,
        Some(Command::Devices(args)) => run_devices(&cli, args).await,
        Some(Command::Apps { json }) => run_apps(&cli, *json).await,
        Some(Command::App { command }) => run_app_command(&cli, command).await,
        Some(Command::Get { channel, json }) => run_get(&cli, channel, *json).await,
        Some(Command::Set(args)) => run_set(&cli, args).await,
        Some(Command::Volume(args)) => run_volume(&cli, args).await,
        Some(Command::Mute(args)) => run_mute(&cli, args).await,
        Some(Command::Unmute(args)) => run_unmute(&cli, args).await,
        Some(Command::Config { command }) => run_config(command),
    }
}

async fn run_doctor(cli: &Cli) -> Result<()> {
    let options = DiscoveryOptions::resolve(cli.core_props.clone());
    let diagnosis = doctor::run(&options, cli.verbose).await;
    print(&diagnosis.report)?;
    diagnosis.outcome
}

async fn run_status(cli: &Cli, json: bool) -> Result<()> {
    let app = build_app(cli)?;
    let routes = app.routes().await?;
    if json {
        print(&output::json_line(&output::status_json(&routes)))?;
    } else {
        print(&output::status_text(&routes))?;
    }
    Ok(())
}

async fn run_devices(cli: &Cli, args: &DevicesArgs) -> Result<()> {
    let app = build_app(cli)?;
    let devices = app.devices(args.role()).await?;
    if args.json {
        print(&output::json_line(&output::devices_json(&devices)))?;
    } else {
        print(&output::devices_text(&devices))?;
    }
    Ok(())
}

async fn run_apps(cli: &Cli, json: bool) -> Result<()> {
    let app = build_app(cli)?;
    let applications = app.applications().await?;
    if json {
        print(&output::json_line(&output::applications_json(
            &applications,
        )))
    } else {
        print(&output::applications_text(&applications))
    }
}

async fn run_app_command(cli: &Cli, command: &AppCommand) -> Result<()> {
    match command {
        AppCommand::Set(args) => run_app_set(cli, args).await,
    }
}

async fn run_app_set(cli: &Cli, args: &AppSetArgs) -> Result<()> {
    let (selector, channel_name) = match (args.pid, &args.channel) {
        (Some(process_id), None) => (
            ApplicationSelector::ProcessId(process_id),
            args.selector.as_str(),
        ),
        (Some(_), Some(_)) => {
            return Err(Error::InvalidArguments(
                "When using --pid, provide only the destination channel after the option."
                    .to_string(),
            ));
        }
        (None, Some(channel)) => (
            ApplicationSelector::Query(args.selector.clone()),
            channel.as_str(),
        ),
        (None, None) => {
            return Err(Error::InvalidArguments(
                "Provide an application name and destination channel, or use --pid <PID> <channel>."
                    .to_string(),
            ));
        }
    };
    let channel: Channel = channel_name.parse()?;
    let app = build_app(cli)?;
    let application = app.set_application_route(&selector, channel).await?;
    if args.json {
        print(&output::json_line(&output::applications_json(&[
            application,
        ])))
    } else {
        print(&output::application_set_text(
            &application,
            std::io::stdout().is_terminal(),
        ))
    }
}

async fn run_get(cli: &Cli, channel: &str, json: bool) -> Result<()> {
    let channel: Channel = channel.parse()?;
    let app = build_app(cli)?;
    let route = app.route(channel).await?;
    if json {
        print(&output::json_line(&output::get_json(&route)))?;
    } else {
        print(&output::get_text(&route))?;
    }
    Ok(())
}

async fn run_set(cli: &Cli, args: &SetArgs) -> Result<()> {
    let channels = parse_channels(&args.channels)?;
    let selector = match (&args.id, &args.device) {
        (Some(id), _) => DeviceSelector::Id(id.clone()),
        (None, Some(device)) => DeviceSelector::Query(device.clone()),
        (None, None) => {
            return Err(Error::InvalidArguments(
                "A device name or --id is required.".to_string(),
            ));
        }
    };

    let app = build_app(cli)?;
    let unicode = std::io::stdout().is_terminal();
    for channel in channels {
        let device = app.set_route(channel, &selector).await?;
        print(&output::set_text(channel, &device, unicode))?;
    }
    Ok(())
}

async fn run_volume(cli: &Cli, args: &VolumeArgs) -> Result<()> {
    let app = build_app(cli)?;
    let states = match (&args.channel, &args.value) {
        (None, None) => app.volumes().await?,
        (None, Some(_)) => {
            return Err(Error::InvalidArguments(
                "A channel is required when changing volume.".to_string(),
            ));
        }
        (Some(channels), None) => {
            let channels = parse_mixer_channels(channels)?;
            let mut states = Vec::with_capacity(channels.len());
            for channel in channels {
                states.push(app.volume(channel).await?);
            }
            states
        }
        (Some(channels), Some(value)) => {
            let channels = parse_mixer_channels(channels)?;
            app.change_volumes(&channels, parse_volume_change(value)?)
                .await?
        }
    };
    print_volumes(&states, args.json)
}

async fn run_mute(cli: &Cli, args: &MuteArgs) -> Result<()> {
    let app = build_app(cli)?;
    let channels = parse_mixer_channels(&args.channels)?;
    let change = match args.action {
        MuteAction::Mute => MuteChange::Set(true),
        MuteAction::Toggle => MuteChange::Toggle,
    };
    let states = app.change_mutes(&channels, change).await?;
    print_volumes(&states, args.json)
}

async fn run_unmute(cli: &Cli, args: &MixerChannelsArgs) -> Result<()> {
    let app = build_app(cli)?;
    let channels = parse_mixer_channels(&args.channels)?;
    let states = app.change_mutes(&channels, MuteChange::Set(false)).await?;
    print_volumes(&states, args.json)
}

fn print_volumes(states: &[sonarctl::sonar::models::VolumeState], json: bool) -> Result<()> {
    if json {
        print(&output::json_line(&output::volumes_json(states)))
    } else {
        print(&output::volumes_text(states))
    }
}

fn run_config(command: &ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Path => match Config::path() {
            Some(path) => {
                print(&format!("{}\n", path.display()))?;
                Ok(())
            }
            None => Err(Error::Other(
                "Could not determine the configuration directory.".to_string(),
            )),
        },
        ConfigCommand::Show => {
            let config = Config::load()?;
            let mut text = String::new();
            match &config.path {
                Some(path) => text.push_str(&format!("# {}\n", path.display())),
                None => text.push_str("# no configuration file (using defaults)\n"),
            }
            text.push_str("[devices]\n");
            for (alias, device) in &config.devices {
                let name = device.name().unwrap_or("");
                match device.id() {
                    Some(id) => text.push_str(&format!(
                        "{alias} = {{ name = \"{name}\", id = \"{id}\" }}\n"
                    )),
                    None => text.push_str(&format!("{alias} = \"{name}\"\n")),
                }
            }
            text.push_str("\n[tui]\n");
            text.push_str(&format!(
                "refresh_interval_ms = {}\n",
                config.tui.refresh_interval_ms
            ));
            print(&text)?;
            Ok(())
        }
    }
}
