use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn a_failed_join_shows_the_step_that_failed() {
    let failed =
        anyhow::anyhow!("the voice server didn't answer address discovery, asked 5 times over 5 s");
    assert_eq!(
        join_failed("1", Step::Connecting, failed).to_string(),
        "Couldn't connect to Discord voice: the voice server didn't answer address discovery, \
         asked 5 times over 5 s."
    );
    // The gateway's errors are sentences of their own.
    let failed = anyhow::anyhow!("Discord disconnected while joining voice.");
    assert_eq!(
        join_failed(PRIVATE, Step::Asking, failed).to_string(),
        "Discord disconnected while joining voice."
    );
}

/// Held by each test that joins a call in the fake Discord, so one runs at
/// a time: they join as the same account, which sees another's join as
/// joining elsewhere and ends its call, and the fake's voice controls are
/// for every call.
static CALLS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Posts to one of the fake Discord's controls.
async fn control(path: &str) -> String {
    let server = yapfast_discord::client::test_server().unwrap();
    let address = server.trim_start_matches("http://").trim_end_matches('/');
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream
        .write_all(
            format!(
                "POST {path} HTTP/1.1\r\nHost: {address}\r\n\
                 Content-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    answer
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn a_call_that_cant_connect_says_why() {
    assert!(
        yapfast_discord::client::test_server().is_some(),
        "set YAPFAST_DISCORD_TEST_SERVER to the fake Discord"
    );
    let _alone = CALLS.lock().await;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let discord = Discord::start(yapfast_discord::token::load().unwrap()).unwrap();
    let mut online = discord.snapshots();
    tokio::time::timeout(
        Duration::from_secs(10),
        online.wait_for(|snapshot| snapshot.status == Status::Online),
    )
    .await
    .expect("the fake Discord answers")
    .unwrap();
    assert!(
        control("/_control/voice-discovery?off")
            .await
            .contains("ignored")
    );
    let (_commands, mut received) = mpsc::unbounded_channel();
    let (snapshots, _states) = watch::channel(State::default());
    let mut state = State::default();
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        call(
            &discord,
            "1400000000000000100",
            "1400000000000000103",
            &mut state,
            &mut received,
            &snapshots,
            None,
        ),
    )
    .await;
    control("/_control/voice-discovery").await;
    // What the voice worker puts in the state the UI shows.
    let error = result.expect("the call gave up").unwrap_err().to_string();
    assert!(
        error.starts_with(
            "Couldn't connect to Discord voice: the voice server didn't answer address discovery"
        ),
        "{error}"
    );
}

/// Discord's client keeps the user's own mute apart from deafening:
/// undeafening gives the microphone back to one who wasn't muted, leaves
/// one who was, and a call joined deafened and unmuted starts talking once
/// it hears.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn undeafening_leaves_the_mute_as_it_was() {
    assert!(
        yapfast_discord::client::test_server().is_some(),
        "set YAPFAST_DISCORD_TEST_SERVER to the fake Discord"
    );
    let _alone = CALLS.lock().await;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let discord = Discord::start(yapfast_discord::token::load().unwrap()).unwrap();
    let mut online = discord.snapshots();
    tokio::time::timeout(
        Duration::from_secs(10),
        online.wait_for(|snapshot| snapshot.status == Status::Online),
    )
    .await
    .expect("the fake Discord answers")
    .unwrap();
    let (commands, mut received) = mpsc::unbounded_channel();
    let (snapshots, mut states) = watch::channel(State::default());
    let mut state = State::default();
    let call = call(
        &discord,
        "1400000000000000100",
        "1400000000000000103",
        &mut state,
        &mut received,
        &snapshots,
        None,
    );
    let steps = async {
        tokio::time::timeout(
            Duration::from_secs(20),
            states.wait_for(|state| state.phase == Phase::Connected),
        )
        .await
        .expect("the call connects")
        .unwrap();
        // Each step's command, and the call after it: muted, deafened.
        // Every one differs from the one before, so none passes unanswered.
        for (step, (command, after)) in [
            (Command::Mute(false), (false, false)),
            (Command::Deafen(true), (true, true)),
            (Command::Deafen(false), (false, false)),
            (Command::Mute(true), (true, false)),
            (Command::Deafen(true), (true, true)),
            (Command::Deafen(false), (true, false)),
            (Command::DeafenUnmuted, (true, true)),
            (Command::Deafen(false), (false, false)),
            // Unmuting while deafened undeafens.
            (Command::Deafen(true), (true, true)),
            (Command::Mute(false), (false, false)),
        ]
        .into_iter()
        .enumerate()
        {
            commands.send(command).unwrap();
            let reached = tokio::time::timeout(
                Duration::from_secs(5),
                states.wait_for(|state| (state.muted, state.deafened) == after),
            )
            .await
            .is_ok();
            let now = states.borrow().clone();
            assert!(
                reached,
                "step {step}: muted {}, deafened {}, error {:?}",
                now.muted, now.deafened, now.error
            );
        }
        commands.send(Command::Leave).unwrap();
    };
    tokio::pin!(call);
    tokio::select! {
        ended = &mut call => panic!("the call ended: {ended:?}"),
        () = steps => {}
    }
    call.await.unwrap();
}
