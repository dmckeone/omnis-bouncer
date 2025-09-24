use axum_server::{tls_rustls::RustlsConfig, Handle};
use reqwest::Client;
use std::{net::SocketAddr, sync::Arc};
use tokio::{join, sync::Notify};
use tracing::{error, info};

use crate::background::run as background_run;
use crate::config::Config;
use crate::database::{create_redis_client, create_redis_pool};
use crate::queue::{QueueControl, QueueEvents};
use crate::servers::{redirect_http_to_https, secure_server};
use crate::signals::shutdown_signal;
use crate::state::AppState;
use crate::upstream::UpstreamPool;
use crate::{control, omnis};

/// Run the app with the given configuration
pub async fn run(
    config: Config,
    shutdown_handle: Handle,
    stream_notify: Arc<Notify>,
    background_notify: Arc<Notify>,
) {
    // Create Redis Pool
    let redis_pool = match create_redis_pool(&config.redis_uri) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to connect to redis: {:?}", e);
            return;
        }
    };

    // Create Redis subscriber client
    let redis_client = match create_redis_client(&config.redis_uri) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to connect to redis subscriber: {:?}", e);
            return;
        }
    };

    // Create queue control and initialize functions
    let queue = match QueueControl::new(
        redis_pool,
        config.quarantine_expiry,
        config.validated_expiry,
        config.publish_throttle,
    ) {
        Ok(q) => q,
        Err(e) => {
            error!("Failed to initialize queue: {:?}", e);
            return;
        }
    };

    // Initialize queue functions
    if let Err(e) = queue
        .init(
            &config.queue_prefix,
            config.queue_enabled,
            config.store_capacity,
        )
        .await
    {
        error!("Failed to initialize queue: {:?}", e);
        return;
    };

    // Create queue subscriber, for emitted events
    let queue_subscriber =
        match QueueEvents::from_client(redis_client, &config.queue_prefix, stream_notify.clone())
            .await
        {
            Ok(s) => s,
            Err(error) => {
                error!("Failed to initialize Redis subscriber: {:?}", error);
                return;
            }
        };

    // Create a new http client pool
    let http_client = Client::builder()
        .connect_timeout(config.connect_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .build()
        .expect("Failed to build HTTP client");

    let upstream_pool = UpstreamPool::new(config.sticky_session_timeout);
    upstream_pool.add_upstreams(&config.initial_upstream).await;

    let public_tls_pair = config.public_tls_pair.clone();
    let public_tls = RustlsConfig::from_pem(public_tls_pair.0, public_tls_pair.1)
        .await
        .expect("Failed to read public TLS certificate and key");

    let monitor_tls_pair = config.monitor_tls_pair.clone();
    let monitor_tls = RustlsConfig::from_pem(monitor_tls_pair.0, monitor_tls_pair.1)
        .await
        .expect("Failed to read monitor TLS certificate and key");

    // Create our app state
    let state = AppState::new(
        config,
        stream_notify.clone(),
        queue,
        queue_subscriber,
        upstream_pool,
        http_client,
    );

    // Create apps
    let shutdown_app = shutdown_signal(
        shutdown_handle.clone(),
        stream_notify.clone(),
        background_notify.clone(),
    );

    let control_app = control::router(state.clone());
    let upstream_app = omnis::router(state.clone());
    let background_app = background_run(state.clone(), background_notify.clone());

    let upstream_upgrade_addr = SocketAddr::from(([0, 0, 0, 0], state.config.http_port));
    let upstream_addr = SocketAddr::from(([0, 0, 0, 0], state.config.https_port));
    let control_addr = SocketAddr::from(([0, 0, 0, 0], state.config.control_port));

    info!(
        "HTTP Server running on http://{}:{}",
        upstream_upgrade_addr.ip(),
        upstream_upgrade_addr.port()
    );
    info!(
        "HTTPS Server running on https://{}:{}",
        upstream_addr.ip(),
        upstream_addr.port()
    );
    info!(
        "HTTPS Control Server running on https://{}:{}",
        control_addr.ip(),
        control_addr.port()
    );

    let exit = join!(
        shutdown_app,
        secure_server(
            upstream_addr,
            public_tls,
            shutdown_handle.clone(),
            upstream_app
        ),
        secure_server(
            control_addr,
            monitor_tls,
            shutdown_handle.clone(),
            control_app
        ),
        redirect_http_to_https(
            upstream_upgrade_addr,
            upstream_addr.port(),
            shutdown_handle.clone(),
        ),
        background_app
    );

    // Exit results (ignored)
    if exit.0.is_err() {
        error!("Failed to exit signal handler");
    }
    if exit.1.is_err() {
        error!("Failed to exit upstream server");
    }
    if exit.2.is_err() {
        error!("Failed to exit control server");
    }
    if exit.3.is_err() {
        error!("Failed to exit redirect server");
    }
}
