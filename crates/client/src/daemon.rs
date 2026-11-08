//! The client of a member's daemon.

use std::{path::Path, sync::Arc};

use ome_api::daemon::{DaemonEvent, DaemonMethod};

use crate::{Events, Profile, Result, session::Session};

/// A session with a daemon. Cloning shares the session; calls on clones run concurrently.
#[derive(Clone)]
pub struct DaemonClient {
    session: Arc<Session>,
}

impl DaemonClient {
    /// Connects to `profile`'s daemon.
    ///
    /// # Errors
    /// The profile's daemon is not running, or refuses the token.
    pub async fn discover(profile: &Profile) -> Result<Self> {
        let endpoint = profile.daemon()?;
        Self::connect(&endpoint.socket, &endpoint.token).await
    }

    /// Connects to the daemon's socket and authenticates with `token`.
    ///
    /// # Errors
    /// The daemon is not listening, or refuses the token.
    pub async fn connect(socket: impl AsRef<Path>, token: &str) -> Result<Self> {
        Ok(Self {
            session: Arc::new(Session::open(socket.as_ref(), token).await?),
        })
    }

    /// Calls daemon method `M`.
    ///
    /// # Errors
    /// The daemon's error, or a connection failure.
    pub async fn call<M: DaemonMethod>(&self, params: &M::Params) -> Result<M::Result> {
        self.session.call::<M>(params).await
    }

    /// The daemon's events from now on.
    ///
    /// # Errors
    /// A connection failure.
    pub async fn events(&self) -> Result<Events<DaemonEvent>> {
        self.session.subscribe().await
    }
}
