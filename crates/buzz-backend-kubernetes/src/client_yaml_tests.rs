use super::*;

const FIXTURE_MODE: &str = "BUZZ_KUBE_YAML_FIXTURE_MODE";

#[test]
fn kubeconfig_connect_and_exec_lookup_use_the_selected_context() {
    // Exercise connect's real KUBECONFIG path in a child: PATH/KUBECONFIG
    // changes cannot race sibling tests or read the developer's credentials.
    let directory = std::env::temp_dir().join(format!("buzz-yaml-kube-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("config.yaml");
    for (mode, yaml) in [
        ("valid", "apiVersion: v1\nkind: Config\ncurrent-context: primary\nclusters:\n- name: local\n  cluster: {server: 'http://127.0.0.1:9'}\ncontexts:\n- name: primary\n  context: {cluster: local, user: token-user, namespace: default}\n- name: exec-context\n  context: {cluster: local, user: exec-user}\nusers:\n- name: token-user\n  user: {token: fixture-only}\n- name: exec-user\n  user:\n    exec:\n      apiVersion: client.authentication.k8s.io/v1beta1\n      command: aws\n      args: [eks, get-token, '--cluster-name', 'no']\n      env: [{name: EXAMPLE, value: '001'}]\n"),
        ("malformed", "contexts: [unclosed\n"),
    ] {
        std::fs::write(&path, yaml).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "client::yaml_tests::kubeconfig_fixture_child", "--nocapture"])
            .env("KUBECONFIG", &path)
            .env(FIXTURE_MODE, mode)
            .env_remove("HTTPS_PROXY")
            .env_remove("HTTP_PROXY")
            .env_remove("ALL_PROXY")
            .env_remove("https_proxy")
            .env_remove("http_proxy")
            .env_remove("all_proxy")
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {} {}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        if mode == "valid" {
            for child_mode in ["retry", "exec"] {
            let retry = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "client::yaml_tests::kubeconfig_fixture_child", "--nocapture"])
                .env("KUBECONFIG", &path)
                .env(FIXTURE_MODE, child_mode)
                .env_remove("HTTPS_PROXY").env_remove("HTTP_PROXY").env_remove("ALL_PROXY")
                .env_remove("https_proxy").env_remove("http_proxy").env_remove("all_proxy")
                .output().unwrap();
            assert!(retry.status.success(), "{child_mode}: {} {}", String::from_utf8_lossy(&retry.stdout), String::from_utf8_lossy(&retry.stderr));
            }
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn kubeconfig_fixture_child() {
    let Ok(mode) = std::env::var(FIXTURE_MODE) else {
        return;
    };
    let _ = rustls::crypto::ring::default_provider().install_default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        if mode == "malformed" {
            assert!(connect(Some("primary")).await.err().unwrap().contains("could not load kubeconfig"));
            return;
        }
        if mode == "retry" || mode == "exec" {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let path = std::env::var("KUBECONFIG").unwrap();
            let mut yaml = std::fs::read_to_string(&path).unwrap();
            // A real executable credential plugin, with synthetic credentials:
            // successful auth must reach the request's Authorization header.
            let uses_exec = mode == "exec";
            if uses_exec {
                let args = serde_json::to_string(&["-c", r#"printf '%s' '{"apiVersion":"client.authentication.k8s.io/v1beta1","kind":"ExecCredential","status":{"token":"fixture-exec-token"}}'"#]).unwrap();
                yaml = yaml.replace("user: token-user", "user: exec-user")
                    .replace("command: aws", "command: /bin/sh")
                    .replace("args: [eks, get-token, '--cluster-name', 'no']", &format!("args: {args}"));
            }
            // The preceding retry child may have used another ephemeral port.
            let yaml = yaml.lines().map(|line| {
                if line.contains("cluster: {server:") {
                    format!("  cluster: {{server: 'http://{address}'}}")
                } else { line.to_string() }
            }).collect::<Vec<_>>().join("\n");
            std::fs::write(path, yaml).unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 4096];
                let mut used = 0;
                while !request[..used].windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    assert!(used < request.len(), "fixture request headers exceeded bound");
                    let read = stream.read(&mut request[used..]).await.unwrap();
                    assert_ne!(read, 0, "fixture request ended before headers");
                    used += read;
                }
                if uses_exec {
                    assert!(String::from_utf8_lossy(&request[..used]).to_ascii_lowercase().contains("authorization: bearer fixture-exec-token"));
                }
                let (status, body) = if uses_exec {
                    ("200 OK", r#"{"apiVersion":"v1","kind":"PodList","metadata":{},"items":[]}"#)
                } else {
                    ("503 Service Unavailable", r#"{"kind":"Status","apiVersion":"v1","status":"Failure","message":"fixture unavailable","reason":"ServiceUnavailable","code":503}"#)
                };
                let response = format!("HTTP/1.1 {status}\r\nRetry-After: 600\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
            });
            let client = connect(Some("primary")).await.unwrap();
            let pods = kube::Api::<k8s_openapi::api::core::v1::Pod>::namespaced(client, "default");
            let result = tokio::time::timeout(std::time::Duration::from_secs(2), pods.list(&Default::default()))
                .await.expect("provider must return the API response, not hide Retry-After behind its deadline");
            if uses_exec {
                assert!(result.unwrap().items.is_empty());
            } else {
                assert!(matches!(result.unwrap_err(), kube::Error::Api(response) if response.code == 503));
            }
            server.await.unwrap();
            return;
        }
        // Client construction does not send a request to the fixture endpoint.
        let config = config_for(Some("primary")).await.unwrap();
        assert_eq!(config.read_timeout, Some(std::time::Duration::from_secs(295)));
        let connection = connect(Some("primary")).await;
        assert!(connection.is_ok(), "{:?}", connection.err());
        let exec = exec_plugin_for(Some("exec-context")).unwrap();
        assert_eq!(exec.command.as_deref(), Some("aws"));
        assert_eq!(exec.args.unwrap().last().map(String::as_str), Some("no"));
        assert_eq!(exec.env.unwrap()[0].get("value").map(String::as_str), Some("001"));
        assert!(connect(Some("missing-context")).await.err().unwrap().contains("missing-context"));
    });
}
