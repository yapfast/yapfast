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
