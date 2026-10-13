//! Supervisor for locally spawned sidecar services.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::port::is_port_free;

const PROBE_INTERVAL: Duration = Duration::from_millis(200);
const PROBE_HTTP_TIMEOUT: Duration = Duration::from_secs(1);

/// Description of a sidecar process the supervisor can launch.
#[derive(Debug, Clone)]
pub struct ServiceSpec {
    pub name: String,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub host: String,
    pub port: u16,
    pub health_path: String,
    pub startup_timeout_ms: u64,
    pub log_path: Option<PathBuf>,
}

/// Failures while starting or supervising a sidecar.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("port {0} is already in use")]
    PortInUse(u16),
    #[error("failed to spawn {name}: {msg}")]
    Spawn { name: String, msg: String },
    #[error("{0} exited during startup with code {1:?}{2}")]
    Exited(String, Option<i32>, String),
    #[error("{0} did not become healthy in time{1}")]
    Timeout(String, String),
}

/// Last six non-empty lines of the sidecar log, newline-prefixed when non-empty.
fn log_tail(log_path: Option<&PathBuf>) -> String {
    let Some(path) = log_path else {
        return String::new();
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    format!("\n{}", lines[lines.len().saturating_sub(6)..].join("\n"))
}

/// A running sidecar, killed when dropped or stopped.
pub struct ManagedService {
    spec: ServiceSpec,
    child: tokio::process::Child,
}

impl ManagedService {
    /// Spawn the sidecar and wait until it answers its health endpoint.
    pub async fn start(spec: ServiceSpec) -> Result<Self, RuntimeError> {
        if !is_port_free(&spec.host, spec.port) {
            return Err(RuntimeError::PortInUse(spec.port));
        }

        let mut command = tokio::process::Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let log_file = match &spec.log_path {
            Some(path) => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(path)
                    .ok()
            }
            None => None,
        };
        match log_file {
            Some(file) => match file.try_clone() {
                Ok(stderr_file) => {
                    command.stdout(Stdio::from(file));
                    command.stderr(Stdio::from(stderr_file));
                }
                Err(_) => {
                    command.stdout(Stdio::null());
                    command.stderr(Stdio::null());
                }
            },
            None => {
                command.stdout(Stdio::null());
                command.stderr(Stdio::null());
            }
        }
        let mut child = command.spawn().map_err(|err| RuntimeError::Spawn {
            name: spec.name.clone(),
            msg: err.to_string(),
        })?;

        let client = build_client()?;
        let url = health_url(&spec.host, spec.port, &spec.health_path);
        let deadline = Instant::now() + Duration::from_millis(spec.startup_timeout_ms);

        loop {
            let status = child.try_wait().map_err(|err| RuntimeError::Spawn {
                name: spec.name.clone(),
                msg: err.to_string(),
            })?;
            if let Some(status) = status {
                tracing::warn!(service = %spec.name, ?status, "service exited during startup");
                return Err(RuntimeError::Exited(
                    spec.name.clone(),
                    status.code(),
                    log_tail(spec.log_path.as_ref()),
                ));
            }
            if probe(&client, &url).await {
                tracing::debug!(service = %spec.name, url = %url, "service healthy");
                return Ok(Self { spec, child });
            }
            if Instant::now() >= deadline {
                tracing::warn!(service = %spec.name, url = %url, "service startup timed out");
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(RuntimeError::Timeout(
                    spec.name.clone(),
                    log_tail(spec.log_path.as_ref()),
                ));
            }
            tokio::time::sleep(PROBE_INTERVAL).await;
        }
    }

    /// The spec this service was started from.
    pub fn spec(&self) -> &ServiceSpec {
        &self.spec
    }

    /// Root URL of the service.
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.spec.host, self.spec.port)
    }

    /// Whether the service currently answers its health endpoint with 2xx.
    pub async fn is_healthy(&self) -> bool {
        let Ok(client) = build_client() else {
            return false;
        };
        probe(
            &client,
            &health_url(&self.spec.host, self.spec.port, &self.spec.health_path),
        )
        .await
    }

    /// Kill the process and reap it.
    pub async fn stop(mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

fn build_client() -> Result<reqwest::Client, RuntimeError> {
    reqwest::Client::builder()
        .timeout(PROBE_HTTP_TIMEOUT)
        .build()
        .map_err(|err| RuntimeError::Spawn {
            name: "supervisor".to_string(),
            msg: err.to_string(),
        })
}

async fn probe(client: &reqwest::Client, url: &str) -> bool {
    matches!(client.get(url).send().await, Ok(resp) if resp.status().is_success())
}

fn health_url(host: &str, port: u16, path: &str) -> String {
    format!("http://{host}:{port}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port::find_free_port;
    use std::net::TcpListener;

    /// Serializes every test that binds, probes or starts services:
    /// ephemeral ports handed to one test can otherwise collide with
    /// another test's port assertions while they run in parallel.
    static PORT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn spec(name: &str, program: &str, args: &[&str], port: u16, timeout_ms: u64) -> ServiceSpec {
        ServiceSpec {
            name: name.to_string(),
            program: PathBuf::from(program),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            host: "127.0.0.1".to_string(),
            port,
            health_path: "/".to_string(),
            startup_timeout_ms: timeout_ms,
            log_path: None,
        }
    }

    #[tokio::test]
    async fn is_port_free_tracks_the_listener() {
        let _guard = PORT_LOCK.lock().await;
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!is_port_free("127.0.0.1", port));
        drop(listener);
        let mut free = false;
        for _ in 0..20 {
            if is_port_free("127.0.0.1", port) {
                free = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(free, "port {port} was not released after drop");
    }

    #[tokio::test]
    async fn find_free_port_returns_a_candidate() {
        let _guard = PORT_LOCK.lock().await;
        let port = find_free_port("127.0.0.1", 18100, 18149);
        assert!(matches!(port, Some(p) if (18100..=18149).contains(&p)));
    }

    #[tokio::test]
    async fn starts_probes_and_stops_a_sidecar() {
        let _guard = PORT_LOCK.lock().await;
        let port = find_free_port("127.0.0.1", 18150, 18199).unwrap();
        let service = spec(
            "http",
            "python3",
            &[
                "-m",
                "http.server",
                &port.to_string(),
                "--bind",
                "127.0.0.1",
            ],
            port,
            8000,
        );
        let service = ManagedService::start(service).await.unwrap();
        assert_eq!(service.base_url(), format!("http://127.0.0.1:{port}"));
        assert!(service.is_healthy().await);
        assert_eq!(service.spec().name, "http");
        service.stop().await;

        let mut free = false;
        for _ in 0..20 {
            if is_port_free("127.0.0.1", port) {
                free = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(free, "port {port} was not released after stop");
    }

    #[tokio::test]
    async fn refuses_a_busy_port() {
        let _guard = PORT_LOCK.lock().await;
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let service = spec("http", "python3", &["-m", "http.server"], port, 2000);
        let err = ManagedService::start(service).await.err().unwrap();
        assert!(matches!(err, RuntimeError::PortInUse(p) if p == port));
    }

    #[tokio::test]
    async fn reports_a_process_that_exits() {
        let _guard = PORT_LOCK.lock().await;
        let port = find_free_port("127.0.0.1", 18200, 18249).unwrap();
        let service = spec("false", "false", &[], port, 5000);
        let err = ManagedService::start(service).await.err().unwrap();
        assert!(matches!(err, RuntimeError::Exited(..)));
    }

    #[tokio::test]
    async fn exited_error_carries_the_log_tail() {
        let _guard = PORT_LOCK.lock().await;
        let log = std::env::temp_dir().join(format!("ob-runtime-tail-{}.log", std::process::id()));
        let port = find_free_port("127.0.0.1", 18300, 18349).unwrap();
        let mut service = spec(
            "boom",
            "/bin/sh",
            &["-c", "echo boom >&2; exit 2"],
            port,
            5000,
        );
        service.log_path = Some(log.clone());
        let err = ManagedService::start(service).await.err().unwrap();
        match err {
            RuntimeError::Exited(_, Some(2), tail) => assert!(tail.contains("boom")),
            other => panic!("unexpected error: {other:?}"),
        }
        let _ = std::fs::remove_file(log);
    }

    #[tokio::test]
    async fn reports_a_process_that_never_healths() {
        let _guard = PORT_LOCK.lock().await;
        let port = find_free_port("127.0.0.1", 18250, 18299).unwrap();
        let service = spec("sleep", "sleep", &["5"], port, 600);
        let err = ManagedService::start(service).await.err().unwrap();
        assert!(matches!(err, RuntimeError::Timeout(..)));
    }
}
