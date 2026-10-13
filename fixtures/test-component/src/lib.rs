//! Minimal test component: implements the bridge world so the host can
//! exercise ComponentRuntime end-to-end. Answers `__handshake` with a canned
//! JSON object and echoes `__describe` payloads back.

wit_bindgen::generate!({ world: "bridge", path: "wit" });

struct TestComponent;

impl Guest for TestComponent {
    fn call(name: String, export_name: String, input: Vec<u8>) -> Result<Vec<u8>, String> {
        match export_name.as_str() {
            "__handshake" => {
                let body = format!(
                    r#"{{"protocol_version":"1.0.0","name":"{name}","version":"0.1.0","contribution_flags":{{}},"peer_dependencies":[],"sandbox_policy":null}}"#
                );
                Ok(body.into_bytes())
            }
            "__echo" => Ok(input),
            other => Err(format!("unknown export '{other}'")),
        }
    }
}

export!(TestComponent);
