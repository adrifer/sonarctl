//! Application-level volume and mute behavior.

mod common;

use std::sync::Arc;

use sonarctl::app::{App, MuteChange, VolumeChange};
use sonarctl::config::Config;
use sonarctl::sonar::models::{MixerChannel, parse_classic_volumes};

use common::{MockBackend, fixture_json};

#[tokio::test]
async fn applies_absolute_and_relative_volume_changes() {
    let backend = Arc::new(MockBackend::new());
    let app = App::new(backend.clone(), Config::default());

    let changed = app
        .change_volumes(&[MixerChannel::Game], VolumeChange::Absolute(70.0))
        .await
        .expect("absolute");
    assert_eq!(changed[0].percent(), 70.0);

    let changed = app
        .change_volumes(&[MixerChannel::Game], VolumeChange::Relative(5.0))
        .await
        .expect("relative");
    assert_eq!(changed[0].percent(), 75.0);
    assert_eq!(
        backend.volume_calls.lock().unwrap().as_slice(),
        &[(MixerChannel::Game, 0.7), (MixerChannel::Game, 0.75)]
    );
}

#[tokio::test]
async fn rejects_out_of_range_changes_without_mutating() {
    let backend = Arc::new(MockBackend::new());
    let app = App::new(backend.clone(), Config::default());

    let err = app
        .change_volumes(&[MixerChannel::Game], VolumeChange::Relative(1.0))
        .await
        .unwrap_err();
    assert_eq!(err.exit_code(), 2);
    assert!(backend.volume_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn toggles_and_sets_mute_state() {
    let backend = Arc::new(MockBackend::new());
    let app = App::new(backend.clone(), Config::default());

    let toggled = app
        .change_mutes(&[MixerChannel::Chat], MuteChange::Toggle)
        .await
        .expect("toggle");
    assert!(!toggled[0].muted);

    let muted = app
        .change_mutes(
            &[MixerChannel::Game, MixerChannel::Media],
            MuteChange::Set(true),
        )
        .await
        .expect("mute");
    assert!(muted.iter().all(|state| state.muted));
}

#[test]
fn rejects_missing_and_out_of_range_api_values() {
    let mut missing = fixture_json("volumeSettingsClassic.json");
    missing["devices"]
        .as_object_mut()
        .unwrap()
        .remove("chatRender");
    assert!(parse_classic_volumes(&missing).is_err());

    let mut invalid = fixture_json("volumeSettingsClassic.json");
    invalid["devices"]["game"]["classic"]["volume"] = serde_json::json!(1.01);
    assert!(parse_classic_volumes(&invalid).is_err());
}
