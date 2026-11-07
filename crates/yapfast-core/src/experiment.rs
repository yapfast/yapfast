//! Disposable local integration checks. No real account credentials are accepted here.

use anyhow::{Context, Result, ensure};
use matrix_sdk::{
    Client,
    config::RequestConfig,
    ruma::{
        api::client::{
            account::register,
            room::create_room,
            uiaa::{AuthData, Dummy},
        },
        events::room::message::RoomMessageEventContent,
        serde::Raw,
    },
};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

use crate::{calls, session::Session};

const HOMESERVER: &str = "http://localhost:18008";
const PASSWORD: &str = "talk-test-only";

pub async fn run() -> Result<()> {
    let run_id = Uuid::new_v4().simple().to_string();
    #[cfg(not(test))]
    let directory = PathBuf::from(".local/experiments");
    #[cfg(test)]
    let directory = crate::test_data_directory().join("experiments");
    let root = directory.join(&run_id);
    println!("Experiment: local sessions and encrypted synthetic audio.");
    let registration = Client::builder()
        .homeserver_url(HOMESERVER)
        .request_config(
            RequestConfig::new()
                .timeout(Duration::from_secs(10))
                .retry_limit(0),
        )
        .build()
        .await
        .context("start scripts/test-stack up before running the experiment")?;
    let mut sessions = Vec::new();
    for name in ["alice", "bob", "charlie"] {
        let username = format!("yapfast_{run_id}_{name}");
        let mut request = register::v3::Request::new();
        request.username = Some(username.clone());
        request.password = Some(PASSWORD.into());
        request.inhibit_login = true;
        request.auth = Some(AuthData::Dummy(Dummy::new()));
        registration
            .send(request)
            .await
            .map_err(|_| anyhow::anyhow!("could not create a disposable local account"))?;
        let session = Session::login(root.join(name), HOMESERVER, &username, PASSWORD).await?;
        session.synchronize().await?;
        ensure!(
            session.device_is_trusted().await?,
            "the disposable device did not become cross-signed"
        );
        sessions.push(session);
    }
    println!("PASS: three persistent, cross-signed local devices.");

    let mut request = create_room::v3::Request::new();
    request.name = Some("RatFast encrypted audio experiment".into());
    request.invite = sessions[1..]
        .iter()
        .map(|session| session.client.user_id().unwrap().to_owned())
        .collect();
    request.initial_state = vec![Raw::from_json_string(json!({"type":"m.room.encryption", "state_key":"", "content":{"algorithm":"m.megolm.v1.aes-sha2"}}).to_string())?];
    let room = sessions[0].client.create_room(request).await?;
    for session in &sessions[1..] {
        session.client.join_room_by_id(room.room_id()).await?;
        session.synchronize().await?;
    }
    sessions[0].synchronize().await?;
    let event = room
        .send(RoomMessageEventContent::text_plain(
            "RatFast local persistence check",
        ))
        .await?;
    let room_id = room.room_id().to_owned();
    drop(room);

    let alice = sessions.remove(0);
    let device_id = alice.client.device_id().unwrap().to_owned();
    drop(alice);
    let restored = Session::restore(root.join("alice")).await?;
    restored.synchronize().await?;
    ensure!(
        restored.client.device_id() == Some(device_id.as_ref()),
        "restoration changed the authenticated device"
    );
    ensure!(
        restored.device_is_trusted().await?,
        "restoration lost device trust"
    );
    let room = restored
        .client
        .get_room(&room_id)
        .context("restoration lost the encrypted room")?;
    let decrypted = room.event(&event.response.event_id, None).await?;
    ensure!(
        decrypted.raw().get_field::<String>("type")?.as_deref() == Some("m.room.message"),
        "old encrypted message could not be decrypted after restart"
    );
    println!("PASS: restart preserves device identity, trust and message keys.");

    let recovery_key = restored
        .client
        .encryption()
        .recovery()
        .enable()
        .await
        .map_err(|_| anyhow::anyhow!("could not enable recovery for the disposable account"))?;
    let username = restored.client.user_id().unwrap().localpart();
    let second =
        Session::login(root.join("alice-recovery"), HOMESERVER, username, PASSWORD).await?;
    second.synchronize().await?;
    ensure!(
        !second.device_is_trusted().await?,
        "a new device was unexpectedly trusted before recovery"
    );
    let unverified_room = second
        .client
        .get_room(&room_id)
        .context("the new device has not loaded its room")?;
    ensure!(
        calls::require_trusted_encrypted_room(&unverified_room)
            .await
            .is_err(),
        "an unverified device was allowed into encrypted voice"
    );
    second.recover(&recovery_key).await?;
    ensure!(
        second.device_is_trusted().await?,
        "recovery did not cross-sign the new device"
    );
    calls::require_trusted_encrypted_room(&unverified_room).await?;
    println!("PASS: a second device becomes trusted through recovery.");
    second.logout().await?;

    let plaintext = restored
        .client
        .create_room(create_room::v3::Request::new())
        .await?;
    ensure!(
        calls::require_trusted_encrypted_room(&plaintext)
            .await
            .is_err(),
        "an unencrypted room was allowed into encrypted voice"
    );
    plaintext.leave().await?;
    println!("PASS: unverified devices and unencrypted rooms are refused before RTC join.");

    sessions.insert(0, restored);
    // Sliding Sync's sticky-events extension drives call membership and media keys.
    run_calls(&sessions, &room_id).await?;
    println!("Disposable stores retained under {}", root.display());
    Ok(())
}

pub async fn resume(root: PathBuf) -> Result<()> {
    let mut sessions = Vec::new();
    for name in ["alice", "bob", "charlie"] {
        let session = Session::restore_local(root.join(name)).await?;
        ensure!(
            session.client.homeserver().as_str().trim_end_matches('/') == HOMESERVER,
            "resume accepts only local test-stack accounts"
        );
        sessions.push(session);
    }
    let room_id = sessions[0]
        .client
        .rooms()
        .into_iter()
        .find(|room| room.name().as_deref() == Some("RatFast encrypted audio experiment"))
        .context("no saved experiment room")?
        .room_id()
        .to_owned();
    run_calls(&sessions, &room_id).await
}

async fn run_calls(sessions: &[Session], room_id: &matrix_sdk::ruma::RoomId) -> Result<()> {
    let mut sync_tasks = Vec::new();
    for session in sessions {
        sync_tasks.push(session.start_sync().await?);
    }
    let leases = sessions.iter().map(Session::lease).collect();
    let result = calls::run_local_experiment(leases, room_id.to_owned()).await;
    for task in sync_tasks {
        task.stop().await;
    }
    let report = result?;
    println!("PASS: {}", serde_json::to_string(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires scripts/test-stack up; creates disposable local accounts"]
    fn local_sessions_and_encrypted_audio() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(180), run())
                    .await
                    .context("local integration experiment timed out")?
            })
            .unwrap();
    }
}
