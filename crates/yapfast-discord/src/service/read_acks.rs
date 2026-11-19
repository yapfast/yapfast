//! Acknowledging reads on Discord: reads within a second go together, as
//! one acknowledgement for a single channel or one bulk acknowledgement for
//! several, such as a whole server marked read.

use std::{collections::HashMap, pin::Pin, sync::Arc};

use tokio::time::{Duration, Instant, Sleep};

use crate::api::{ACK_BULK_LIMIT, Api};

/// Reads within this long go to Discord as one acknowledgement per channel.
const ACK_DELAY: Duration = Duration::from_secs(1);

pub(super) struct ReadAcks {
    api: Arc<Api>,
    /// The newest read message per channel, waiting to be acknowledged.
    waiting: HashMap<String, String>,
    at: Pin<Box<Sleep>>,
}

impl ReadAcks {
    pub(super) fn new(api: Arc<Api>) -> Self {
        Self {
            api,
            waiting: HashMap::new(),
            at: Box::pin(tokio::time::sleep(ACK_DELAY)),
        }
    }

    /// Acknowledges `message` in `channel` with the next batch.
    pub(super) fn read(&mut self, channel: String, message: String) {
        if self.waiting.is_empty() {
            self.at.as_mut().reset(Instant::now() + ACK_DELAY);
        }
        self.waiting.insert(channel, message);
    }

    /// Drops a read of `channel` still waiting, as one marked unread since.
    pub(super) fn forget(&mut self, channel: &str) {
        self.waiting.remove(channel);
    }

    pub(super) fn waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Sends the batch once it's due.
    pub(super) async fn send_when_due(&mut self) {
        self.at.as_mut().await;
        let reads: Vec<(String, String)> = self.waiting.drain().collect();
        if let [(channel, message)] = reads.as_slice() {
            let (api, channel, message) = (self.api.clone(), channel.clone(), message.clone());
            tokio::spawn(async move {
                if let Err(error) = api.ack(&channel, &message).await {
                    tracing::warn!(%error, "could not mark a Discord channel read");
                }
            });
            return;
        }
        for batch in reads.chunks(ACK_BULK_LIMIT) {
            let (api, batch) = (self.api.clone(), batch.to_vec());
            tokio::spawn(async move {
                if let Err(error) = api.ack_bulk(&batch).await {
                    tracing::warn!(%error, "could not mark Discord channels read");
                }
            });
        }
    }
}
