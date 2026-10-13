//! How a client reads a registry's answer (ADR 0044 D2 as amended by ADR 0075): the call's body, the protocol's
//! error answer, an answer by its status alone, or not a registry's answer.

use serde::de::DeserializeOwned;

use crate::{ErrorBody, ErrorCode, ErrorDetail};

/// Why a 404 is not a registry's: the text a client quotes.
pub const NOT_A_REGISTRY: &str =
    "a 404 without the registry protocol's NOT_FOUND error: no registry serves this address";

/// An answer that is not the call's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The registry's error answer: an error status whose body is the protocol's error body. At 404 its code is
    /// `NOT_FOUND` (a 404 with another body is [`Failure::NotRegistry`]).
    Error { status: u16, error: ErrorDetail },
    /// An error status other than 404 whose body is not the protocol's: read by its status alone (a proxy's 401, a
    /// gateway's 502, a framework's own 400).
    Status { status: u16 },
    /// Not a registry's answer: a 404 without the protocol's `NOT_FOUND` error (an address no registry serves, such
    /// as a URL that isn't a registry's root), or a success whose body isn't the call's wire type (`why` is the parse
    /// error).
    NotRegistry { status: u16, why: String },
}

/// `body`, answered with `status`, as `T`, the call's wire type: a 2xx whose body reads as `T`. Any other answer is
/// the [`Failure`] it is ([`failure`] for an error status).
pub fn read<T: DeserializeOwned>(status: u16, body: &[u8]) -> Result<T, Failure> {
    if !(200..300).contains(&status) {
        return Err(failure(status, body));
    }
    serde_json::from_slice(body).map_err(|e| Failure::NotRegistry {
        status,
        why: e.to_string(),
    })
}

/// What an error status means (a download, whose success body is bytes, asks this for a non-2xx answer).
pub fn failure(status: u16, body: &[u8]) -> Failure {
    let error = serde_json::from_slice::<ErrorBody>(body)
        .ok()
        .map(|body| body.error);
    match (status, error) {
        (404, Some(error)) if error.code == ErrorCode::NotFound => Failure::Error { status, error },
        (404, _) => Failure::NotRegistry {
            status,
            why: NOT_A_REGISTRY.to_string(),
        },
        (_, Some(error)) => Failure::Error { status, error },
        (_, None) => Failure::Status { status },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VersionList;

    fn body(code: ErrorCode) -> Vec<u8> {
        serde_json::to_vec(&ErrorBody::new(code, "m")).unwrap()
    }

    #[specforge_test_macros::test(
        behavior = "check_registry_reply",
        verify = "a reply that is not the protocol's JSON is refused with R009, even with --allow-unsigned"
    )]
    fn an_answer_is_its_wire_type_or_not_a_registrys() {
        let list: VersionList = read(200, br#"{"name":"@a/b","versions":[]}"#).unwrap();
        assert_eq!(list.name, "@a/b");
        for (status, text) in [
            (200, &b"<html>"[..]),
            (201, br#"{"name":"@a/b"}"#.as_slice()),
        ] {
            match read::<VersionList>(status, text).unwrap_err() {
                Failure::NotRegistry { status: s, why } => {
                    assert_eq!(s, status);
                    assert!(!why.is_empty());
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[specforge_test_macros::test(
        behavior = "check_registry_reply",
        verify = "a 404 without the protocol's NOT_FOUND error is R009, not package not found"
    )]
    fn a_404_is_not_found_only_with_the_protocols_error() {
        assert!(matches!(
            failure(404, &body(ErrorCode::NotFound)),
            Failure::Error { status: 404, .. }
        ));
        for text in [
            &b""[..],
            b"<html>",
            br#"{"error":{"message":"x"}}"#,
            &body(ErrorCode::Forbidden),
        ] {
            assert_eq!(
                failure(404, text),
                Failure::NotRegistry {
                    status: 404,
                    why: NOT_A_REGISTRY.to_string()
                },
                "{}",
                String::from_utf8_lossy(text)
            );
        }
    }

    #[specforge_test_macros::test(
        behavior = "authenticate_registry_request",
        verify = "every registry call reads an answer's status as one error"
    )]
    fn another_error_status_reads_by_its_status_without_the_envelope() {
        assert_eq!(failure(401, b""), Failure::Status { status: 401 });
        assert!(matches!(
            failure(400, &body(ErrorCode::UnsignedPackage)),
            Failure::Error { status: 400, .. }
        ));
        assert_eq!(failure(502, b"<html>"), Failure::Status { status: 502 });
        assert_eq!(
            read::<VersionList>(429, b"").unwrap_err(),
            failure(429, b"")
        );
    }
}
