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

/// One call in `guild`'s `channel` until it connects, then left; the next
/// call starts straight after, as switching calls leaves one and joins the
/// next.
async fn join_then_leave(discord: &Discord, guild: &str, channel: &str, what: &str) {
    let (commands, mut received) = mpsc::unbounded_channel();
    let (snapshots, mut states) = watch::channel(State::default());
    let mut state = State::default();
    let call = call(
        discord,
        guild,
        channel,
        &mut state,
        &mut received,
        &snapshots,
        None,
    );
    let steps = async {
        let reached = tokio::time::timeout(
            Duration::from_secs(20),
            states.wait_for(|state| state.phase == Phase::Connected),
        )
        .await
        .is_ok();
        let now = states.borrow().clone();
        assert!(reached, "{what}: the call didn't connect: {:?}", now.error);
        commands.send(Command::Leave).unwrap();
    };
    tokio::pin!(call);
    tokio::select! {
        ended = &mut call => panic!("{what}: the call ended: {ended:?}"),
        () = steps => {}
    }
    call.await.unwrap();
}

/// Switching calls: from a server's voice channel to another of its
/// channels, to a DM's call, back, and again after the gateway resumes; each
/// next call connects.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn switching_calls_joins_each_next_one() {
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
    const GUILD: &str = "1400000000000000100";
    const LOUNGE: &str = "1400000000000000103";
    const ALICES_ROOM: &str = "1400000000000000105";
    const DM_BOB: &str = "1400000000000000120";
    join_then_leave(&discord, GUILD, LOUNGE, "the Lounge").await;
    join_then_leave(
        &discord,
        GUILD,
        ALICES_ROOM,
        "another channel of the server",
    )
    .await;
    join_then_leave(&discord, PRIVATE, DM_BOB, "Bob's DM call").await;
    join_then_leave(&discord, GUILD, LOUNGE, "back to the Lounge").await;
    // Discord asks the gateway to reconnect (op 7): it resumes its session
    // (op 6), and the next joins go through the resumed connection.
    let resumes = || async {
        let answer = control("/_control/sessions").await;
        let body = answer.split("\r\n\r\n").nth(1).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(body).unwrap()["resumes"]
            .as_u64()
            .unwrap()
    };
    let before = resumes().await;
    assert!(
        control("/_control/gateway-reconnect")
            .await
            .contains("reconnect")
    );
    let mut resumed = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if resumes().await > before {
            resumed = true;
            break;
        }
    }
    assert!(resumed, "the gateway resumed");
    join_then_leave(&discord, GUILD, LOUNGE, "the Lounge after a resume").await;
    join_then_leave(&discord, PRIVATE, DM_BOB, "Bob's DM call after a resume").await;
}
