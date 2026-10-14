//! The publisher signature a package carries (ADR 0061): its JSON, the content it covers, how it is made and how
//! it verifies. Both ends run this module. `specforge publish` signs with it, the registry refuses an upload whose
//! signature is not one or does not verify over the upload, and `specforge add` verifies what it downloads again.
//! The registry is still not the trust anchor: which keys a user trusts (pins, `trusted_keys`, `denied_keys`) is the
//! client's.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// SHA-256 of `bytes`, lowercase hex: a package binary's digest as a registry records it (`PackageMetadata::sha256`)
/// and as a signature covers it.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// What a publisher signs for a package: its name and version as its declaration states them, and the SHA-256 of
/// the exact binary and manifest bytes uploaded. A registry builds it from the upload, and a client from the reply
/// and the downloaded bytes. Equal content is what makes a signature verify on both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedContent {
    name: String,
    version: String,
    wasm_sha256: String,
    manifest_sha256: String,
}

impl SignedContent {
    /// The content of `name@version` uploaded as `wasm` with `manifest`, the manifest's exact JSON text. Hashes both
    /// once.
    pub fn of(name: &str, version: &str, wasm: &[u8], manifest: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            wasm_sha256: sha256_hex(wasm),
            manifest_sha256: sha256_hex(manifest.as_bytes()),
        }
    }

    /// The binary's digest, as [`sha256_hex`] computes it: what a registry records as `sha256`, so it hashes the
    /// upload once.
    pub fn wasm_sha256(&self) -> &str {
        &self.wasm_sha256
    }

    /// The bytes signed at `signed_at`: the compact JSON
    /// `{"name","version","wasmSha256","manifestSha256","signedAt"}`, in that order. The signed bytes, unchanged
    /// since round 3.
    fn payload(&self, signed_at: &str) -> Vec<u8> {
        /// Field order is byte order: serde writes a struct's fields in declaration order.
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Payload<'a> {
            name: &'a str,
            version: &'a str,
            wasm_sha256: &'a str,
            manifest_sha256: &'a str,
            signed_at: &'a str,
        }
        serde_json::to_vec(&Payload {
            name: &self.name,
            version: &self.version,
            wasm_sha256: &self.wasm_sha256,
            manifest_sha256: &self.manifest_sha256,
            signed_at,
        })
        .expect("a payload of strings serializes")
    }
}

/// A publisher's Ed25519 signature over a package's [`SignedContent`], with the public key and the time that verify
/// it offline.
///
/// JSON (a publish form's `signature` part, and `PackageMetadata::signature`), with no other key:
/// `{"sig": <128 hex>, "public_key": <64 hex>, "signed_at": <RFC 3339>}`.
///
/// Only a well-formed one exists: [`PackageSignature::parse`] and deserializing refuse anything else, and the only
/// other way to make one is [`PackageSignature::sign`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SignatureJson", into = "SignatureJson")]
pub struct PackageSignature {
    sig: Signature,
    public_key: VerifyingKey,
    /// RFC 3339, kept as signed: the payload covers this exact text.
    signed_at: String,
}

impl PackageSignature {
    /// `key`'s signature over `content` at `at` (written with `to_rfc3339`).
    pub fn sign(
        key: &SigningKey,
        content: &SignedContent,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let signed_at = at.to_rfc3339();
        Self {
            sig: key.sign(&content.payload(&signed_at)),
            public_key: key.verifying_key(),
            signed_at,
        }
    }

    /// The signature `json` holds: a publish form's `signature` part as the registry receives it. Every
    /// [`SignatureError`] but `DoesNotVerify`.
    pub fn parse(json: &str) -> Result<Self, SignatureError> {
        let shape: SignatureJson =
            serde_json::from_str(json).map_err(|why| SignatureError::Shape(why.to_string()))?;
        Self::try_from(shape)
    }

    /// Its JSON text: what a publish form's `signature` part carries and a registry stores.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&SignatureJson::from(self.clone())).expect("three strings serialize")
    }

    /// The signing key's id, the first 16 hex characters of SHA-256 of the public key: what a user pins and what
    /// W156, R-TRUST-003 and `specforge.lock` name.
    pub fn key_id(&self) -> String {
        key_id(&self.public_key)
    }

    /// When it was signed, as signed (RFC 3339).
    pub fn signed_at(&self) -> &str {
        &self.signed_at
    }

    /// `Ok` when it is its public key's signature over `content` (strict Ed25519 verification); else
    /// [`SignatureError::DoesNotVerify`].
    pub fn verify(&self, content: &SignedContent) -> Result<(), SignatureError> {
        self.public_key
            .verify_strict(&content.payload(&self.signed_at), &self.sig)
            .map_err(|_| SignatureError::DoesNotVerify)
    }
}

/// The id of `public_key`: the first 16 hex characters of its SHA-256. A publisher's `SigningKey::key_id` and
/// [`PackageSignature::key_id`] are this one function.
pub fn key_id(public_key: &VerifyingKey) -> String {
    sha256_hex(public_key.as_bytes())[..16].to_string()
}

/// Why a text is not a package signature, or why one does not verify. Its `Display` is the sentence a registry
/// refusal and a client diagnostic carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureError {
    /// Not a JSON object of exactly the strings `sig`, `public_key` and `signed_at`; the JSON reader's reason.
    Shape(String),
    /// `sig` is not 128 hex characters.
    Sig,
    /// `public_key` is not 64 hex characters of an Ed25519 point.
    PublicKey,
    /// `public_key` is a small-order point, which "verifies" signatures it never made.
    WeakPublicKey,
    /// `signed_at` is not an RFC 3339 timestamp.
    SignedAt,
    /// Well-formed, but not its key's signature over the content.
    DoesNotVerify,
}

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape(why) => write!(
                f,
                "the publisher signature is not a JSON object of `sig`, `public_key` and `signed_at`: {why}"
            ),
            Self::Sig => f.write_str(
                "the publisher signature's `sig` is not 128 hex characters (an Ed25519 signature)",
            ),
            Self::PublicKey => f.write_str(
                "the publisher signature's `public_key` is not 64 hex characters of an Ed25519 public key",
            ),
            Self::WeakPublicKey => f.write_str(
                "the publisher signature's `public_key` is a weak (small-order) Ed25519 key",
            ),
            Self::SignedAt => f.write_str(
                "the publisher signature's `signed_at` is not an RFC 3339 timestamp",
            ),
            Self::DoesNotVerify => f.write_str(
                "the publisher signature is not its key's signature over this name, version, binary and manifest",
            ),
        }
    }
}

impl std::error::Error for SignatureError {}

/// The JSON shape. `deny_unknown_fields`, so the round-3 envelope (`keyId`, `pubkey`, `signedAt`) is refused by
/// name.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignatureJson {
    sig: String,
    public_key: String,
    signed_at: String,
}

impl TryFrom<SignatureJson> for PackageSignature {
    type Error = SignatureError;

    fn try_from(json: SignatureJson) -> Result<Self, SignatureError> {
        let sig: [u8; 64] = hex::decode(&json.sig)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(SignatureError::Sig)?;
        let public_key: [u8; 32] = hex::decode(&json.public_key)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(SignatureError::PublicKey)?;
        let public_key =
            VerifyingKey::from_bytes(&public_key).map_err(|_| SignatureError::PublicKey)?;
        if public_key.is_weak() {
            return Err(SignatureError::WeakPublicKey);
        }
        chrono::DateTime::parse_from_rfc3339(&json.signed_at)
            .map_err(|_| SignatureError::SignedAt)?;
        Ok(Self {
            sig: Signature::from_bytes(&sig),
            public_key,
            signed_at: json.signed_at,
        })
    }
}

impl From<PackageSignature> for SignatureJson {
    fn from(signature: PackageSignature) -> Self {
        Self {
            sig: hex::encode(signature.sig.to_bytes()),
            public_key: hex::encode(signature.public_key.as_bytes()),
            signed_at: signature.signed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const NAME: &str = "@acme/greeter";
    const VERSION: &str = "1.2.0";
    const WASM: &[u8] = b"\0asm greeter";
    const MANIFEST: &str = r#"{"handshake":{"name":"@acme/greeter","version":"1.2.0"}}"#;

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn at() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 10, 11, 0, 0, 0).unwrap()
    }

    fn content() -> SignedContent {
        SignedContent::of(NAME, VERSION, WASM, MANIFEST)
    }

    #[test]
    fn the_payload_is_deterministic_and_ordered() {
        let at = "2026-09-24T00:00:00+00:00";
        assert_eq!(content().payload(at), content().payload(at));
        let text = String::from_utf8(content().payload(at)).unwrap();
        // Field order is byte order: name < version < wasmSha256 < manifestSha256 < signedAt.
        let positions: Vec<usize> = [
            "\"name\"",
            "\"version\"",
            "\"wasmSha256\"",
            "\"manifestSha256\"",
            "\"signedAt\"",
        ]
        .iter()
        .map(|key| text.find(key).unwrap())
        .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{text}");
    }

    #[test]
    fn the_payload_changes_when_any_field_changes() {
        let at = "2026-09-24T00:00:00+00:00";
        let base = content().payload(at);
        for other in [
            SignedContent::of("@acme/other", VERSION, WASM, MANIFEST),
            SignedContent::of(NAME, "9.9.9", WASM, MANIFEST),
            SignedContent::of(NAME, VERSION, b"\0asm other", MANIFEST),
            SignedContent::of(NAME, VERSION, WASM, "{}"),
        ] {
            assert_ne!(base, other.payload(at), "{other:?}");
        }
        assert_ne!(base, content().payload("2027-01-01T00:00:00+00:00"));
    }

    #[test]
    fn a_signature_verifies_over_its_content() {
        let signature = PackageSignature::sign(&key(), &content(), at());
        signature.verify(&content()).expect("verifies");
        assert_eq!(signature.key_id().len(), 16);
        assert_eq!(signature.key_id(), key_id(&key().verifying_key()));
        assert_eq!(signature.signed_at(), "2026-10-11T00:00:00+00:00");
    }

    #[test]
    fn a_signature_does_not_verify_over_other_content() {
        let signature = PackageSignature::sign(&key(), &content(), at());
        for other in [
            SignedContent::of(NAME, VERSION, b"\0asm other", MANIFEST),
            SignedContent::of(NAME, VERSION, WASM, "{}"),
            SignedContent::of("@acme/other", VERSION, WASM, MANIFEST),
            SignedContent::of(NAME, "1.2.1", WASM, MANIFEST),
        ] {
            assert_eq!(
                signature.verify(&other),
                Err(SignatureError::DoesNotVerify),
                "{other:?}"
            );
        }
    }

    #[test]
    fn different_keys_have_different_ids() {
        let a = SigningKey::from_bytes(&[7; 32]).verifying_key();
        let b = SigningKey::from_bytes(&[8; 32]).verifying_key();
        assert_ne!(key_id(&a), key_id(&b));
    }

    /// What `SigningKey::sign_package` wrote before the envelope changed (round 3 to round 7), for the key
    /// `[7; 32]` over [`content`] at `2026-09-24T00:00:00+00:00`.
    const ROUND_3_PAYLOAD: &str = r#"{"name":"@acme/greeter","version":"1.2.0","wasmSha256":"73f404b2241beae4ffd7f337f0aee0ce6a6c6c5afdaf7bf2569a673e8a49f017","manifestSha256":"1ce1931132cf580deef76750c19cb605bbe32542a995d0489104603aef505302","signedAt":"2026-09-24T00:00:00+00:00"}"#;
    const ROUND_3_SIG: &str = "f7522b429b4e2480ea9bcc0ff09abf33d8d54c2b0517b0874e29aa12ff8df06f55b61a8b0f91ce08381288a76c362ee4a536e6163f5989d8094e8116705f890f";
    const ROUND_3_PUBLIC_KEY: &str =
        "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";

    #[test]
    fn the_payload_is_the_bytes_publishers_have_signed_since_round_3() {
        assert_eq!(
            String::from_utf8(content().payload("2026-09-24T00:00:00+00:00")).unwrap(),
            ROUND_3_PAYLOAD
        );
        let signature = PackageSignature::parse(&format!(
            r#"{{"sig":"{ROUND_3_SIG}","public_key":"{ROUND_3_PUBLIC_KEY}","signed_at":"2026-09-24T00:00:00+00:00"}}"#
        ))
        .unwrap();
        signature
            .verify(&content())
            .expect("a round-3 signature verifies");
        assert_eq!(signature.key_id(), "fe812c12f3ab4ce6");
    }

    /// The JSON of a signature with `field` replaced by `value` (a string), or removed when `None`.
    fn with(field: &str, value: Option<&str>) -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(&PackageSignature::sign(&key(), &content(), at()).to_json())
                .unwrap();
        let object = json.as_object_mut().unwrap();
        match value {
            Some(value) => object.insert(field.to_string(), value.into()),
            None => object.remove(field),
        };
        json.to_string()
    }

    #[specforge_test_macros::test(
        type = "PackageSignature",
        verify = "PackageSignature is the JSON of a publisher signature, and only a well-formed one reads"
    )]
    fn a_package_signature_is_this_json() {
        let signature = PackageSignature::sign(&key(), &content(), at());
        let json: serde_json::Value = serde_json::from_str(&signature.to_json()).unwrap();
        let object = json.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["public_key", "sig", "signed_at"]);
        let hex_of = |key: &str, len: usize| {
            let text = object[key].as_str().unwrap();
            assert_eq!(text.len(), len, "{key}");
            assert!(
                text.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{key}"
            );
        };
        hex_of("sig", 128);
        hex_of("public_key", 64);
        assert_eq!(object["signed_at"], "2026-10-11T00:00:00+00:00");
        assert_eq!(
            PackageSignature::parse(&signature.to_json()).unwrap(),
            signature
        );
        assert_eq!(
            serde_json::to_value(&signature).unwrap(),
            json,
            "serde writes the same JSON"
        );

        let round_3 = format!(
            r#"{{"sig":"{ROUND_3_SIG}","keyId":"fe812c12f3ab4ce6","pubkey":"{ROUND_3_PUBLIC_KEY}","signedAt":"2026-09-24T00:00:00+00:00"}}"#
        );
        let identity = format!("01{}", "00".repeat(31));
        // Each text and the variant it is refused with (`Shape` with a part of the reader's reason).
        let shape = |why: &str| SignatureError::Shape(why.to_string());
        let refused = [
            ("hello".to_string(), shape("expected value")),
            (round_3, shape("unknown field `keyId`")),
            (with("sig", Some("x")), SignatureError::Sig),
            (with("sig", Some(&"ab".repeat(63))), SignatureError::Sig),
            (with("public_key", Some("z")), SignatureError::PublicKey),
            (
                with("public_key", Some(&identity)),
                SignatureError::WeakPublicKey,
            ),
            (with("signed_at", Some("now")), SignatureError::SignedAt),
            (with("signed_at", None), shape("missing field `signed_at`")),
        ];
        for (text, expected) in refused {
            let error = PackageSignature::parse(&text).unwrap_err();
            match (&error, &expected) {
                (SignatureError::Shape(why), SignatureError::Shape(part)) => {
                    assert!(why.contains(part.as_str()), "{text}: {why}")
                }
                _ => assert_eq!(error, expected, "{text}"),
            }
            assert!(
                serde_json::from_str::<PackageSignature>(&text).is_err(),
                "{text} deserializes"
            );
        }
    }

    #[test]
    fn sha256_hex_is_lowercase_hex() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
