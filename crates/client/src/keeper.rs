//! The client of a member's keeper.

use std::{path::Path, sync::Arc};

use ome_api::keeper::{KeeperEvent, KeeperMethod};

use crate::{Events, Result, session::Session};

/// A session with a keeper. Cloning shares the session; calls on clones run concurrently.
#[derive(Clone)]
pub struct KeeperClient {
    session: Arc<Session>,
}

impl KeeperClient {
    /// Connects to the keeper's socket and authenticates with `token`.
    ///
    /// # Errors
    /// The keeper is not listening, or refuses the token.
    pub async fn connect(socket: impl AsRef<Path>, token: &str) -> Result<Self> {
        Ok(Self {
            session: Arc::new(Session::open(socket.as_ref(), token).await?),
        })
    }

    /// Calls keeper method `M`.
    ///
    /// # Errors
    /// The keeper's error, or a connection failure.
    pub async fn call<M: KeeperMethod>(&self, params: &M::Params) -> Result<M::Result> {
        self.session.call::<M>(params).await
    }

    /// The keeper's events from now on.
    ///
    /// # Errors
    /// A connection failure.
    pub async fn events(&self) -> Result<Events<KeeperEvent>> {
        self.session.subscribe().await
    }
}
