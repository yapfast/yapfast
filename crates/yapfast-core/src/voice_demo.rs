//! Disposable local accounts and a persistent server for manual voice testing.
use std::path::PathBuf;

use anyhow::{Context, Result};
use matrix_sdk::{
    Client,
    ruma::{
        api::client::{
            account::register,
            room::create_room,
            uiaa::{AuthData, Dummy},
        },
        serde::Raw,
    },
};
use serde_json::json;

use crate::session::Session;

pub async fn provision() -> Result<PathBuf> {
    #[cfg(not(test))]
    let directory = PathBuf::from(".local/voice-demo");
    #[cfg(test)]
    let directory = crate::test_data_directory().join("voice-demo");
    let root = directory.join(uuid::Uuid::new_v4().simple().to_string());
    let registration = Client::builder()
        .homeserver_url("http://localhost:18008")
        .build()
        .await?;
    let mut accounts = Vec::new();
    for name in ["alice", "bob"] {
        let username = format!("yapfast_voice_{}_{}", uuid::Uuid::new_v4().simple(), name);
        let mut request = register::v3::Request::new();
        request.username = Some(username.clone());
        request.password = Some("talk-test-only".into());
        request.inhibit_login = true;
        request.auth = Some(AuthData::Dummy(Dummy::new()));
        registration
            .send(request)
            .await
            .context("Start scripts/test-stack up before creating the voice demo.")?;
        let session = Session::login(
            root.join(name),
            "http://localhost:18008",
            &username,
            "talk-test-only",
        )
        .await?;
        session.synchronize().await?;
        accounts.push(session);
    }
    let mut channels = Vec::new();
    for (name, voice) in [("general", false), ("Lounge", true)] {
        let mut request = create_room::v3::Request::new();
        request.name = Some(name.into());
        request.invite = vec![accounts[1].client.user_id().unwrap().to_owned()];
        request.initial_state = vec![Raw::from_json_string(json!({"type":"m.room.encryption", "state_key":"", "content":{"algorithm":"m.megolm.v1.aes-sha2"}}).to_string())?];
        if voice {
            request.creation_content = Some(Raw::from_json_string(
                json!({"type":"org.matrix.msc3417.call"}).to_string(),
            )?);
        }
        let room = accounts[0].client.create_room(request).await?;
        if voice {
            matrix_rtc_livekit::open_slot(
                &accounts[0].client,
                room.room_id().as_str(),
                "m.call#ROOM",
                "m.call",
                Some(matrix_rtc_core::SlotEncryption {
                    encryption_type: "m.per_member".into(),
                    extra: Default::default(),
                }),
            )
            .await?;
        }
        accounts[1].client.join_room_by_id(room.room_id()).await?;
        channels.push(room.room_id().to_owned());
    }
    let mut request = create_room::v3::Request::new();
    request.name = Some("RatFast voice test".into());
    request.creation_content = Some(Raw::from_json_string(
        json!({"type":"m.space"}).to_string(),
    )?);
    request.invite = vec![accounts[1].client.user_id().unwrap().to_owned()];
    request.initial_state = channels
        .iter()
        .map(|id| {
            Raw::from_json_string(
                json!({
                    "type":"m.space.child", "state_key":id, "content":{"via":["synapse"]}
                })
                .to_string(),
            )
        })
        .collect::<Result<_, _>>()?;
    let space = accounts[0].client.create_room(request).await?;
    accounts[1].client.join_room_by_id(space.room_id()).await?;
    for account in &accounts {
        account.synchronize().await?;
    }
    let manifest = json!({"space":space.room_id(), "text":channels[0], "voice":channels[1],
        "alice":accounts[0].client.user_id(), "bob":accounts[1].client.user_id()});
    std::fs::write(
        root.join("rooms.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    println!("Voice demo ready. No microphone or speaker has been opened.");
    println!("Open the two local test accounts in separate processes:");
    println!(
        "cargo run --locked -- --data-dir {}",
        root.join("alice").display()
    );
    println!(
        "cargo run --locked -- --data-dir {}",
        root.join("bob").display()
    );
    println!(
        "Select Yapfast voice test, then Lounge, and Join voice. Both join with the microphone on; \
         mute one, or use headphones."
    );
    Ok(root)
}
