use axum::extract::Request;
use axum::{
    handler::HandlerWithoutStateExt, http::{uri::Authority, uri::Scheme, StatusCode, Uri}, response::Redirect,
    BoxError,
    Router,
    ServiceExt,
};
use axum_extra::extract::Host;
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use std::io;
use std::net::SocketAddr;

/// Create an insecure server from an Axum router
pub async fn insecure_server(
    addr: SocketAddr,
    shutdown_handle: Handle,
    router: Router,
) -> io::Result<()> {
    let server = axum_server::bind(addr);
    let service = ServiceExt::<Request>::into_make_service(router);
    server.handle(shutdown_handle).serve(service).await
}

/// Create a secure server from an Axum router
pub async fn secure_server(
    addr: SocketAddr,
    tls_config: RustlsConfig,
    shutdown_handle: Handle,
    router: Router,
) -> io::Result<()> {
    let mut server = axum_server::bind_rustls(addr, tls_config);
    // Advertise support for HTTP/2 to the client (required by web sockets)
    server.http_builder().http2().enable_connect_protocol();
    let service = ServiceExt::<Request>::into_make_service(router);

    server.handle(shutdown_handle).serve(service).await
}

// Construct a redirect URI
fn make_https(host: &str, uri: Uri, https_port: u16) -> Result<Uri, BoxError> {
    let mut parts = uri.into_parts();

    parts.scheme = Some(Scheme::HTTPS);

    if parts.path_and_query.is_none() {
        parts.path_and_query = Some("/".parse().unwrap());
    }

    let authority: Authority = host.parse()?;
    let bare_host = match authority.port() {
        Some(port_struct) => authority
            .as_str()
            .strip_suffix(port_struct.as_str())
            .unwrap()
            .strip_suffix(':')
            .unwrap(), // if authority.port() is Some(port) then we can be sure authority ends with :{port}
        None => authority.as_str(),
    };

    parts.authority = Some(format!("{bare_host}:{https_port}").parse()?);

    Ok(Uri::from_parts(parts)?)
}

/// Server that only redirects http to https
pub async fn redirect_http_to_https(
    http_port: u16,
    https_port: u16,
    shutdown_handle: Handle,
) -> anyhow::Result<()> {
    let redirect = move |Host(host): Host, uri: Uri| async move {
        match make_https(&host, uri, https_port) {
            Ok(uri) => Ok(Redirect::to(&uri.to_string())),
            Err(error) => {
                tracing::warn!(%error, "failed to convert URI to HTTPS");
                Err(StatusCode::BAD_REQUEST)
            }
        }
    };

    // Listen on Socket addr/port
    let addr = SocketAddr::from(([0, 0, 0, 0], http_port));

    // Start Axum server
    let mut server = axum_server::bind(addr);

    // Advertise support for HTTP/2 to the client (required by web sockets)
    server.http_builder().http2().enable_connect_protocol();

    // DEV NOTE: For further options, see: https://docs.rs/hyper-util/0.1.11/hyper_util/server/conn/auto/struct.Builder.html
    server
        .handle(shutdown_handle)
        .serve(redirect.into_make_service())
        .await?;

    Ok(())
}
