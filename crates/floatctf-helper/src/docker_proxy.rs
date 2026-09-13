use std::{convert::Infallible, os::unix::fs::PermissionsExt, path::Path, sync::Arc};

use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Full, combinators::BoxBody};
use hyper::{
    Method, Request, Response, StatusCode,
    body::Incoming,
    header::{CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, UPGRADE},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::{client::legacy::Client, rt::TokioIo};
use hyperlocal::{UnixClientExt, UnixConnector, Uri as UnixUri};
use serde_json::{Map, Value};
use tokio::net::UnixListener;

const MAX_POLICY_BODY: usize = 2 * 1024 * 1024;
const MANAGED_LABEL: &str = "floatctf.managed";

type ProxyBody = BoxBody<Bytes, hyper::Error>;
type DockerClient = Client<UnixConnector, ProxyBody>;

pub async fn serve(socket_path: &str, backend_path: &str) -> anyhow::Result<()> {
    let socket = Path::new(socket_path);
    if let Some(parent) = socket.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if socket.exists() {
        tokio::fs::remove_file(socket).await?;
    }
    let listener = UnixListener::bind(socket)?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o660))?;

    let client: DockerClient = Client::unix();
    let backend: Arc<str> = Arc::from(backend_path.to_string());
    eprintln!("floatctf-helper Docker policy proxy listening on {socket_path} -> {backend_path}");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let client = client.clone();
        let backend = backend.clone();
        tokio::spawn(async move {
            let service =
                service_fn(move |request| proxy_request(request, client.clone(), backend.clone()));
            if let Err(error) = http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await
            {
                eprintln!("helper Docker proxy connection failed: {error}");
            }
        });
    }
}

async fn proxy_request(
    request: Request<Incoming>,
    client: DockerClient,
    backend: Arc<str>,
) -> Result<Response<ProxyBody>, Infallible> {
    let normalized = normalize_path(request.uri().path()).to_string();
    let method = request.method().clone();

    let policy = authorize_and_prepare(request, &client, &backend, &normalized, &method).await;
    let request = match policy {
        Ok(request) => request,
        Err((status, message)) => return Ok(error_response(status, &message)),
    };

    let upgrade_requested = request.headers().contains_key(UPGRADE)
        || request
            .headers()
            .get(CONNECTION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_ascii_lowercase().contains("upgrade"));

    let mut request = request;
    let downstream_upgrade = upgrade_requested.then(|| hyper::upgrade::on(&mut request));
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or("/")
        .to_string();
    *request.uri_mut() = UnixUri::new(backend.as_ref(), &path_and_query).into();

    match client.request(request).await {
        Ok(mut response) => {
            if upgrade_requested && response.status() == StatusCode::SWITCHING_PROTOCOLS {
                let upstream_upgrade = hyper::upgrade::on(&mut response);
                if let Some(downstream_upgrade) = downstream_upgrade {
                    tokio::spawn(async move {
                        let Ok(downstream) = downstream_upgrade.await else {
                            return;
                        };
                        let Ok(upstream) = upstream_upgrade.await else {
                            return;
                        };
                        let mut downstream = TokioIo::new(downstream);
                        let mut upstream = TokioIo::new(upstream);
                        let _ = tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await;
                    });
                }
            }
            Ok(response.map(|body| body.boxed()))
        }
        Err(error) => Ok(error_response(
            StatusCode::BAD_GATEWAY,
            &format!("Docker backend unavailable: {error}"),
        )),
    }
}

async fn authorize_and_prepare(
    request: Request<Incoming>,
    client: &DockerClient,
    backend: &str,
    path: &str,
    method: &Method,
) -> Result<Request<ProxyBody>, (StatusCode, String)> {
    // Host-wide read-only information used by bootstrap/admin status pages.
    if matches!(
        (method, path),
        (&Method::GET, "/_ping")
            | (&Method::HEAD, "/_ping")
            | (&Method::GET, "/version")
            | (&Method::GET, "/info")
            | (&Method::GET, "/system/df")
            | (&Method::GET, "/containers/json")
            | (&Method::GET, "/images/json")
            | (&Method::GET, "/networks")
    ) {
        return Ok(box_request(request));
    }

    // Pull/build are creation operations. Existing-image mutation is ownership-gated below.
    if path == "/build" && method == Method::POST
        || path == "/images/create" && method == Method::POST
    {
        return Ok(box_request(request));
    }

    if path.starts_with("/images/") {
        if method == Method::GET {
            return Ok(box_request(request));
        }
        if let Some(image) = image_mutation_target(path, method) {
            ensure_managed_image(client, backend, image).await?;
            return Ok(box_request(request));
        }
        return Err((
            StatusCode::FORBIDDEN,
            format!("Docker image operation denied: {method} {path}"),
        ));
    }

    if path == "/containers/create" && method == Method::POST {
        let (parts, body) = collect_request(request).await?;
        let body = validate_container_create(body)?;
        return Ok(request_with_bytes(parts, body));
    }

    if path == "/networks/create" && method == Method::POST {
        let (parts, body) = collect_request(request).await?;
        let body = validate_network_create(body)?;
        return Ok(request_with_bytes(parts, body));
    }

    if let Some((container, suffix)) = container_target(path) {
        let allowed = match (method, suffix) {
            (&Method::GET, "/json" | "/logs" | "/stats" | "/top" | "/archive") => true,
            (&Method::HEAD, "/archive") => true,
            (&Method::PUT, "/archive") => true,
            (&Method::POST, "/start" | "/stop" | "/restart" | "/wait" | "/exec") => true,
            (&Method::DELETE, "") => true,
            _ => false,
        };
        if !allowed {
            return Err((
                StatusCode::FORBIDDEN,
                format!("Docker operation denied: {method} {path}"),
            ));
        }
        ensure_managed_container(client, backend, container).await?;
        return Ok(box_request(request));
    }

    if let Some((exec_id, suffix)) = exec_target(path) {
        if !matches!(
            (method, suffix),
            (&Method::POST, "/start" | "/resize") | (&Method::GET, "/json")
        ) {
            return Err((
                StatusCode::FORBIDDEN,
                format!("Docker exec operation denied: {method} {path}"),
            ));
        }
        let container = exec_container(client, backend, exec_id).await?;
        ensure_managed_container(client, backend, &container).await?;
        return Ok(box_request(request));
    }

    if let Some((network, suffix)) = network_target(path) {
        if !matches!(
            (method, suffix),
            (&Method::GET, "")
                | (&Method::DELETE, "")
                | (&Method::POST, "/connect" | "/disconnect")
        ) {
            return Err((
                StatusCode::FORBIDDEN,
                format!("Docker network operation denied: {method} {path}"),
            ));
        }
        ensure_managed_network(client, backend, network).await?;
        if method == Method::POST && matches!(suffix, "/connect" | "/disconnect") {
            let (parts, body) = collect_request(request).await?;
            let value: Value = serde_json::from_slice(&body).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("invalid Docker network body: {e}"),
                )
            })?;
            let container = value
                .get("Container")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    (
                        StatusCode::BAD_REQUEST,
                        "Docker network request missing Container".to_string(),
                    )
                })?;
            ensure_managed_container(client, backend, container).await?;
            return Ok(request_with_bytes(parts, body));
        }
        return Ok(box_request(request));
    }

    Err((
        StatusCode::FORBIDDEN,
        format!("Docker API operation is outside FloatCTF helper policy: {method} {path}"),
    ))
}

fn box_request(request: Request<Incoming>) -> Request<ProxyBody> {
    request.map(|body| body.boxed())
}

async fn collect_request(
    request: Request<Incoming>,
) -> Result<(hyper::http::request::Parts, Bytes), (StatusCode, String)> {
    let (parts, body) = request.into_parts();
    let collected = body.collect().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("read Docker request body: {e}"),
        )
    })?;
    let bytes = collected.to_bytes();
    if bytes.len() > MAX_POLICY_BODY {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "Docker policy body too large".to_string(),
        ));
    }
    Ok((parts, bytes))
}

fn request_with_bytes(mut parts: hyper::http::request::Parts, bytes: Bytes) -> Request<ProxyBody> {
    // container/network create 会注入 helper ownership label，body 长度发生变化。
    // 必须同步 Content-Length，避免 Docker daemon 按原始长度读取导致 JSON 截断。
    parts.headers.remove(CONTENT_LENGTH);
    if let Ok(value) = hyper::header::HeaderValue::from_str(&bytes.len().to_string()) {
        parts.headers.insert(CONTENT_LENGTH, value);
    }
    Request::from_parts(
        parts,
        Full::new(bytes).map_err(|never| match never {}).boxed(),
    )
}

fn validate_container_create(body: Bytes) -> Result<Bytes, (StatusCode, String)> {
    let mut value: Value = serde_json::from_slice(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid Docker create body: {e}"),
        )
    })?;
    let root = value.as_object_mut().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Docker create body must be an object".to_string(),
        )
    })?;

    let host = root.get("HostConfig").and_then(Value::as_object);
    if let Some(host) = host {
        deny_bool(host, "Privileged")?;
        deny_nonempty(host, "Binds")?;
        deny_nonempty(host, "Mounts")?;
        deny_nonempty(host, "Devices")?;
        deny_nonempty(host, "DeviceRequests")?;
        deny_nonempty(host, "CapAdd")?;
        deny_nonempty(host, "SecurityOpt")?;
        deny_nonempty(host, "Sysctls")?;
        deny_nonempty(host, "VolumesFrom")?;
        for key in [
            "PidMode",
            "IpcMode",
            "UTSMode",
            "CgroupnsMode",
            "UsernsMode",
        ] {
            if host
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty())
            {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("Docker create forbids HostConfig.{key}"),
                ));
            }
        }
        if let Some(mode) = host.get("NetworkMode").and_then(Value::as_str) {
            if mode == "host" || mode.starts_with("container:") {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Docker create forbids host/container network mode".to_string(),
                ));
            }
            if !mode.is_empty()
                && mode != "default"
                && mode != "bridge"
                && mode != "none"
                && !is_floatctf_network_name(mode)
            {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("Docker create network is outside FloatCTF namespace: {mode}"),
                ));
            }
        }
    }

    if let Some(endpoints) = root
        .get("NetworkingConfig")
        .and_then(Value::as_object)
        .and_then(|networking| networking.get("EndpointsConfig"))
        .and_then(Value::as_object)
    {
        for network in endpoints.keys() {
            if !is_floatctf_network_name(network) {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("Docker create endpoint is outside FloatCTF namespace: {network}"),
                ));
            }
        }
    }

    let labels = root
        .entry("Labels")
        .or_insert_with(|| Value::Object(Map::new()));
    let labels = labels.as_object_mut().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Docker Labels must be an object".to_string(),
        )
    })?;
    labels.insert(MANAGED_LABEL.to_string(), Value::String("true".to_string()));

    serde_json::to_vec(&value).map(Bytes::from).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("encode Docker create body: {e}"),
        )
    })
}

fn validate_network_create(body: Bytes) -> Result<Bytes, (StatusCode, String)> {
    let mut value: Value = serde_json::from_slice(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid Docker network body: {e}"),
        )
    })?;
    let root = value.as_object_mut().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Docker network body must be an object".to_string(),
        )
    })?;
    let name = root.get("Name").and_then(Value::as_str).unwrap_or("");
    if !is_floatctf_network_name(name) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Docker network name is outside FloatCTF namespace: {name}"),
        ));
    }
    if root
        .get("Driver")
        .and_then(Value::as_str)
        .is_some_and(|v| !v.is_empty() && v != "bridge")
    {
        return Err((
            StatusCode::FORBIDDEN,
            "FloatCTF helper only permits bridge networks".to_string(),
        ));
    }
    let labels = root
        .entry("Labels")
        .or_insert_with(|| Value::Object(Map::new()));
    let labels = labels.as_object_mut().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Docker network Labels must be an object".to_string(),
        )
    })?;
    labels.insert(MANAGED_LABEL.to_string(), Value::String("true".to_string()));
    serde_json::to_vec(&value).map(Bytes::from).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("encode Docker network body: {e}"),
        )
    })
}

fn is_floatctf_network_name(name: &str) -> bool {
    name.starts_with("fctf-") || name.starts_with("floatctf-")
}

fn deny_bool(map: &Map<String, Value>, key: &str) -> Result<(), (StatusCode, String)> {
    if map.get(key).and_then(Value::as_bool) == Some(true) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Docker create forbids HostConfig.{key}=true"),
        ));
    }
    Ok(())
}

fn deny_nonempty(map: &Map<String, Value>, key: &str) -> Result<(), (StatusCode, String)> {
    let nonempty = match map.get(key) {
        Some(Value::Array(v)) => !v.is_empty(),
        Some(Value::Object(v)) => !v.is_empty(),
        Some(Value::String(v)) => !v.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    };
    if nonempty {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Docker create forbids HostConfig.{key}"),
        ));
    }
    Ok(())
}

async fn ensure_managed_container(
    client: &DockerClient,
    backend: &str,
    id: &str,
) -> Result<(), (StatusCode, String)> {
    let value = backend_json(client, backend, &format!("/containers/{id}/json")).await?;
    let managed = value
        .pointer("/Config/Labels")
        .and_then(Value::as_object)
        .is_some_and(|labels| {
            labels.get(MANAGED_LABEL).and_then(Value::as_str) == Some("true")
                || labels.get("io.floatctf.managed").and_then(Value::as_str) == Some("true")
                || labels.contains_key("awd.resource_kind")
        });
    if !managed {
        return Err((
            StatusCode::FORBIDDEN,
            format!("container is not FloatCTF-managed: {id}"),
        ));
    }
    Ok(())
}

async fn ensure_managed_image(
    client: &DockerClient,
    backend: &str,
    image: &str,
) -> Result<(), (StatusCode, String)> {
    let value = backend_json(client, backend, &format!("/images/{image}/json")).await?;
    let managed = value
        .pointer("/Config/Labels")
        .and_then(Value::as_object)
        .is_some_and(|labels| {
            labels.get(MANAGED_LABEL).and_then(Value::as_str) == Some("true")
                || labels.get("io.floatctf.managed").and_then(Value::as_str) == Some("true")
        });
    if !managed {
        return Err((
            StatusCode::FORBIDDEN,
            format!("image is not FloatCTF-managed: {image}"),
        ));
    }
    Ok(())
}

async fn ensure_managed_network(
    client: &DockerClient,
    backend: &str,
    id: &str,
) -> Result<(), (StatusCode, String)> {
    let value = backend_json(client, backend, &format!("/networks/{id}")).await?;
    let managed_by_label = value
        .get("Labels")
        .and_then(Value::as_object)
        .and_then(|labels| labels.get(MANAGED_LABEL))
        .and_then(Value::as_str)
        == Some("true");
    let managed_by_name = value
        .get("Name")
        .and_then(Value::as_str)
        .is_some_and(is_floatctf_network_name);
    let managed = managed_by_label || managed_by_name;
    if !managed {
        return Err((
            StatusCode::FORBIDDEN,
            format!("network is not FloatCTF-managed: {id}"),
        ));
    }
    Ok(())
}

async fn exec_container(
    client: &DockerClient,
    backend: &str,
    exec_id: &str,
) -> Result<String, (StatusCode, String)> {
    let value = backend_json(client, backend, &format!("/exec/{exec_id}/json")).await?;
    value
        .get("ContainerID")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                format!("Docker exec has no container: {exec_id}"),
            )
        })
}

async fn backend_json(
    client: &DockerClient,
    backend: &str,
    path: &str,
) -> Result<Value, (StatusCode, String)> {
    let uri: hyper::Uri = UnixUri::new(backend, path).into();
    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(
            Empty::<Bytes>::new()
                .map_err(|never| match never {})
                .boxed(),
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let response = client.request(request).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Docker backend inspect failed: {e}"),
        )
    })?;
    if !response.status().is_success() {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Docker object is unavailable or unmanaged: {path}"),
        ));
    }
    let body = response
        .into_body()
        .collect()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Docker backend inspect body failed: {e}"),
            )
        })?
        .to_bytes();
    serde_json::from_slice(&body).map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Docker backend returned invalid JSON: {e}"),
        )
    })
}

fn normalize_path(path: &str) -> &str {
    if let Some(rest) = path.strip_prefix("/v") {
        if let Some(pos) = rest.find('/') {
            let version = &rest[..pos];
            if version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            {
                return &rest[pos..];
            }
        }
    }
    path
}

fn container_target(path: &str) -> Option<(&str, &str)> {
    split_target(path, "/containers/")
}

fn exec_target(path: &str) -> Option<(&str, &str)> {
    split_target(path, "/exec/")
}

fn image_mutation_target<'a>(path: &'a str, method: &Method) -> Option<&'a str> {
    let rest = path.strip_prefix("/images/")?;
    if method == Method::DELETE && !rest.is_empty() && !rest.contains('/') {
        return Some(rest);
    }
    if method == Method::POST {
        for suffix in ["/tag", "/push"] {
            if let Some(image) = rest.strip_suffix(suffix)
                && !image.is_empty()
            {
                return Some(image);
            }
        }
    }
    None
}

fn network_target(path: &str) -> Option<(&str, &str)> {
    if path == "/networks" || path == "/networks/create" {
        return None;
    }
    split_target(path, "/networks/")
}

fn split_target<'a>(path: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let rest = path.strip_prefix(prefix)?;
    if rest.is_empty() {
        return None;
    }
    match rest.find('/') {
        Some(pos) => Some((&rest[..pos], &rest[pos..])),
        None => Some((rest, "")),
    }
}

fn error_response(status: StatusCode, message: &str) -> Response<ProxyBody> {
    let body = serde_json::json!({"message": message}).to_string();
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json")
        .body(
            Full::new(Bytes::from(body))
                .map_err(|never| match never {})
                .boxed(),
        )
        .expect("valid helper error response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_docker_api_version() {
        assert_eq!(normalize_path("/v1.47/containers/json"), "/containers/json");
        assert_eq!(normalize_path("/containers/json"), "/containers/json");
    }

    #[test]
    fn container_policy_rejects_privileged_and_mounts() {
        let body = Bytes::from_static(br#"{"Image":"x","HostConfig":{"Privileged":true}}"#);
        assert!(validate_container_create(body).is_err());
        let body = Bytes::from_static(br#"{"Image":"x","HostConfig":{"Binds":["/:/host"]}}"#);
        assert!(validate_container_create(body).is_err());
    }

    #[test]
    fn container_policy_injects_managed_label() {
        let body = Bytes::from_static(br#"{"Image":"x","HostConfig":{"Privileged":false}}"#);
        let body = validate_container_create(body).unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value
                .pointer("/Labels/floatctf.managed")
                .and_then(Value::as_str),
            Some("true")
        );
    }

    #[test]
    fn container_policy_limits_network_namespace() {
        let bad =
            Bytes::from_static(br#"{"Image":"x","HostConfig":{"NetworkMode":"private-net"}}"#);
        assert!(validate_container_create(bad).is_err());
        let good = Bytes::from_static(
            br#"{"Image":"x","HostConfig":{"NetworkMode":"fctf-awd-deadbeef"}}"#,
        );
        assert!(validate_container_create(good).is_ok());
        let endpoint = Bytes::from_static(
            br#"{"Image":"x","NetworkingConfig":{"EndpointsConfig":{"postgres_default":{}}}}"#,
        );
        assert!(validate_container_create(endpoint).is_err());
    }

    #[test]
    fn image_mutations_are_narrowly_identified() {
        assert_eq!(
            image_mutation_target("/images/sha256:abc", &Method::DELETE),
            Some("sha256:abc")
        );
        assert_eq!(
            image_mutation_target("/images/sha256:abc/tag", &Method::POST),
            Some("sha256:abc")
        );
        assert_eq!(
            image_mutation_target("/images/repo:tag/push", &Method::POST),
            Some("repo:tag")
        );
        assert_eq!(
            image_mutation_target("/images/repo:tag/json", &Method::POST),
            None
        );
    }

    #[test]
    fn network_policy_limits_namespace_and_driver() {
        assert!(
            validate_network_create(Bytes::from_static(
                br#"{"Name":"bridge","Driver":"bridge"}"#
            ))
            .is_err()
        );
        assert!(
            validate_network_create(Bytes::from_static(
                br#"{"Name":"fctf-awd-1","Driver":"macvlan"}"#
            ))
            .is_err()
        );
        let ok = validate_network_create(Bytes::from_static(
            br#"{"Name":"fctf-awd-1","Driver":"bridge"}"#,
        ))
        .unwrap();
        let value: Value = serde_json::from_slice(&ok).unwrap();
        assert_eq!(
            value
                .pointer("/Labels/floatctf.managed")
                .and_then(Value::as_str),
            Some("true")
        );
    }
}
