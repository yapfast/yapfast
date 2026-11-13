mod store;

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use matrix_sdk::{
    Client,
    config::RequestConfig,
    cross_process_lock::CrossProcessLockConfig,
    encryption::{BackupDownloadStrategy, EncryptionSettings},
};
use matrix_sdk_ui::sync_service::SyncService;
use uuid::Uuid;

use store::{SavedSession, SecretStorage, SessionStore};

pub struct Session {
    pub client: Client,
    store: Arc<SessionStore>,
    id: Uuid,
    storage: SecretStorage,
    passphrase: String,
}

/// Keep the process lock alive wherever a background consumer owns the client.
pub(crate) struct SessionLease {
    pub client: Client,
    _store: Arc<SessionStore>,
}

impl Session {
    pub(crate) fn lease(&self) -> SessionLease {
        SessionLease {
            client: self.client.clone(),
            _store: self.store.clone(),
        }
    }
    pub fn storage_notice(&self) -> &'static str {
        self.storage.notice()
    }
    pub async fn login(
        root: PathBuf,
        homeserver: &str,
        username: &str,
        password: &str,
    ) -> Result<Self> {
        let store = open_store(root).await?;
        let check = store.clone();
        let storage = tokio::task::spawn_blocking(move || {
            if check.load()?.is_some() {
                bail!("this directory already has a session; restore it or log out first")
            }
            check.choose_storage()
        })
        .await??;
        println!("{}", storage.notice());
        let id = Uuid::new_v4();
        let passphrase = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let check = store.clone();
        let key = passphrase.clone();
        let path = tokio::task::spawn_blocking(move || {
            let path = check.prepare_sdk_store(id)?;
            check.save_store_key(id, storage, &key)?;
            anyhow::Ok(path)
        })
        .await??;
        let client = build_client(homeserver, path, &passphrase).await?;
        let flows = client
            .matrix_auth()
            .get_login_types()
            .await
            .map_err(|_| anyhow::anyhow!("could not check the homeserver's sign-in methods"))?;
        if !flows
            .flows
            .iter()
            .any(|flow| flow.login_type() == "m.login.password")
        {
            bail!(
                "this homeserver requires SSO or OAuth sign-in; this build supports password sign-in only"
            )
        }
        client
            .matrix_auth()
            .login_username(username, password)
            .initial_device_display_name("Yapfast")
            .send()
            .await
            .map_err(|_| {
                anyhow::anyhow!("sign-in failed; check the homeserver, username and password")
            })?;
        let session = Self {
            client,
            store,
            id,
            storage,
            passphrase,
        };
        session.persist().await?;
        session
            .client
            .encryption()
            .wait_for_e2ee_initialization_tasks()
            .await;
        Ok(session)
    }

    pub async fn restore(root: PathBuf) -> Result<Self> {
        Self::restore_from(root, None).await
    }

    pub(crate) async fn restore_local(root: PathBuf) -> Result<Self> {
        Self::restore_from(root, Some("http://localhost:18008")).await
    }

    async fn restore_from(root: PathBuf, expected_homeserver: Option<&str>) -> Result<Self> {
        let store = open_store(root).await?;
        let check = store.clone();
        let (id, storage, saved) = tokio::task::spawn_blocking(move || check.load())
            .await??
            .context("no saved session in this directory; sign in first")?;
        println!("{}", storage.notice());
        if let Some(expected) = expected_homeserver
            && saved.homeserver.trim_end_matches('/') != expected
        {
            bail!("the experiment accepts only local test-stack accounts")
        }
        let client = build_client(&saved.homeserver, store.sdk_path(id), &saved.passphrase).await?;
        client.matrix_auth().restore_session(saved.session, Default::default()).await.map_err(|_| anyhow::anyhow!("could not restore the saved device; preserve the crypto store before repairing the session"))?;
        client
            .encryption()
            .wait_for_e2ee_initialization_tasks()
            .await;
        Ok(Self {
            client,
            store,
            id,
            storage,
            passphrase: saved.passphrase,
        })
    }

    pub async fn persist(&self) -> Result<()> {
        let saved = SavedSession {
            homeserver: self.client.homeserver().to_string(),
            passphrase: self.passphrase.clone(),
            session: self
                .client
                .matrix_auth()
                .session()
                .context("the client has no authenticated session")?,
        };
        let store = self.store.clone();
        let id = self.id;
        let storage = self.storage;
        tokio::task::spawn_blocking(move || store.save(id, storage, &saved)).await?
    }

    pub async fn synchronize(&self) -> Result<()> {
        let sync = self.start_sync().await?;
        let result = sync.wait_until_ready().await;
        sync.stop().await;
        result?;
        let user = self.client.user_id().context("not signed in")?;
        self.client
            .encryption()
            .request_user_identity(user)
            .await
            .map_err(|_| anyhow::anyhow!("could not refresh this account's encryption identity"))?;
        Ok(())
    }

    pub async fn start_sync(&self) -> Result<SessionSync> {
        // Offline mode keeps probing the homeserver after a network failure and
        // resumes by itself; Yapfast then re-enables the paused send queue.
        let service = SyncService::builder(self.client.clone())
            .with_offline_mode()
            .build()
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "could not start Matrix sync; this homeserver needs Simplified Sliding Sync"
                )
            })?;
        service.start().await;
        Ok(SessionSync {
            service: Arc::new(service),
            store: Some(self.store.clone()),
        })
    }

    pub async fn recover(&self, key: &str) -> Result<()> {
        self.client
            .encryption()
            .recovery()
            .recover(key)
            .await
            .map_err(|_| {
                anyhow::anyhow!("recovery failed; check the recovery key and homeserver connection")
            })?;
        self.synchronize().await?;
        self.persist().await
    }

    pub async fn device_is_trusted(&self) -> Result<bool> {
        let user = self.client.user_id().context("not signed in")?;
        let device = self.client.device_id().context("not signed in")?;
        let own = self.client.encryption().get_device(user, device).await?;
        Ok(own.is_some_and(|device| device.is_verified_with_cross_signing()))
    }

    pub async fn logout(&self) -> Result<()> {
        // Do not discard local credentials when server-side revocation failed.
        self.client.matrix_auth().logout().await.map_err(|_| {
            anyhow::anyhow!("logout failed; credentials were kept so you can retry")
        })?;
        let store = self.store.clone();
        let id = self.id;
        let storage = self.storage;
        tokio::task::spawn_blocking(move || store.clear(id, storage)).await??;
        Ok(())
    }
}

pub struct SessionSync {
    service: Arc<SyncService>,
    store: Option<Arc<SessionStore>>,
}
impl SessionSync {
    pub fn states(
        &self,
    ) -> impl futures_util::Stream<Item = matrix_sdk_ui::sync_service::State> + use<> {
        use futures_util::StreamExt;
        let states = self.service.state();
        let initial = states.get();
        futures_util::stream::once(async move { initial }).chain(states)
    }
    pub async fn stop(mut self) {
        self.service.stop().await;
        self.store.take();
    }
    pub async fn wait_until_ready(&self) -> Result<()> {
        use matrix_sdk_ui::room_list_service::State;
        let rooms = self.service.room_list_service();
        let mut state = rooms.state();
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match state.get() {
                    State::SettingUp | State::Running => return Ok(()),
                    State::Error { .. } | State::Terminated { .. } => {
                        bail!("Matrix sync stopped; check homeserver support and connection")
                    }
                    _ => {}
                }
                if state.next().await.is_none() {
                    bail!("Matrix sync stopped before it loaded the account")
                }
            }
        })
        .await
        .context("Matrix sync did not load the account before its deadline")?
    }
}
impl Drop for SessionSync {
    fn drop(&mut self) {
        if let Some(store) = self.store.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let service = self.service.clone();
            runtime.spawn(async move {
                service.stop().await;
                drop(store);
            });
        }
    }
}

async fn open_store(root: PathBuf) -> Result<Arc<SessionStore>> {
    tokio::task::spawn_blocking(move || SessionStore::open(root).map(Arc::new)).await?
}

async fn build_client(homeserver: &str, path: PathBuf, passphrase: &str) -> Result<Client> {
    let url = url::Url::parse(homeserver).context("invalid homeserver URL")?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        bail!("homeservers require HTTPS; HTTP is allowed only for the local development stack")
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("homeserver URLs must not contain credentials")
    }
    // Keep the URL checked above: a login response must not move Yapfast to
    // another, possibly plain-HTTP, homeserver address. The session lock keeps
    // the stores to this process, so the SDK's cross-process lock, which
    // rewrites its lease in the crypto store every 50 ms, stays off.
    Client::builder().homeserver_url(homeserver).respect_login_well_known(false).sqlite_store(path, Some(passphrase))
        .cross_process_store_config(CrossProcessLockConfig::SingleProcess)
        .request_config(RequestConfig::new().timeout(Duration::from_secs(15)).retry_limit(1))
        .with_room_key_recipient_strategy(matrix_sdk_crypto::CollectStrategy::IdentityBasedStrategy)
        .with_encryption_settings(EncryptionSettings {
            auto_enable_cross_signing: true,
            auto_enable_backups: false,
            backup_download_strategy: BackupDownloadStrategy::AfterDecryptionFailure,
        })
        .build().await.map_err(|_| anyhow::anyhow!("could not open the Matrix client and encrypted stores; check the homeserver URL and preserve existing stores"))
}
