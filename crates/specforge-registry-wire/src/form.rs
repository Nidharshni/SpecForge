//! The multipart form a publish uploads (`PUT` [`crate::address::Call::Version`]).

/// The extension declaration, as JSON: the package's manifest (ADR 0012).
pub const MANIFEST: &str = "manifest";
/// The component binary.
pub const WASM: &str = "wasm";
/// The publisher signature object, when signed.
pub const SIGNATURE: &str = "signature";

/// `multipart/form-data; boundary={boundary}`.
pub fn content_type(boundary: &str) -> String {
    format!("multipart/form-data; boundary={boundary}")
}

/// The whole body: the manifest part (`application/json`), the binary part (`application/wasm`, filename
/// `extension.wasm`) and, when given, the signature part, then the closing boundary. Built explicitly,
/// so its length is known before it is sent (`30f82d2f`).
pub fn body(boundary: &str, manifest: &str, wasm: &[u8], signature: Option<&str>) -> Vec<u8> {
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{MANIFEST}\"\r\nContent-Type: application/json\r\n\r\n{manifest}\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{WASM}\"; filename=\"extension.wasm\"\r\nContent-Type: application/wasm\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wasm);
    body.extend_from_slice(b"\r\n");
    if let Some(signature) = signature {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{SIGNATURE}\"\r\n\r\n{signature}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}
