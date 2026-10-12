//! The notifications the server sends, typed. Their frames are MCP's; the
//! SpecForge deltas' params are messages (`crate::message`).

use serde::Serialize;
use serde_json::Value;
use specforge_project::GraphDelta;

use super::DiagnosticsDelta;

/// One notification queued for the client.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "method", content = "params")]
pub enum Notification {
    /// `subscriptions/listen` honoured these resources (MCP 2026-07-28).
    #[serde(rename = "notifications/subscriptions/acknowledged")]
    Acknowledged(Acknowledged),
    /// A resource the client hears about changed.
    #[serde(rename = "notifications/resources/updated")]
    ResourceUpdated(ResourceUpdated),
    /// The graph delta, whole (handshake subscribers of a graph view).
    #[serde(rename = "specforge/graphChanged")]
    GraphChanged(GraphDelta),
    /// The diagnostics added and removed (handshake subscribers of
    /// `specforge://diagnostics`).
    #[serde(rename = "specforge/diagnosticsChanged")]
    DiagnosticsChanged(DiagnosticsDelta),
}

impl Notification {
    /// `notifications/subscriptions/acknowledged` for the stream `id`,
    /// naming the resources honoured.
    pub(super) fn acknowledged(id: &Value, uris: &[String]) -> Self {
        Notification::Acknowledged(Acknowledged {
            meta: StreamMeta {
                subscription_id: id.clone(),
            },
            notifications: Honoured {
                resource_subscriptions: uris.to_vec(),
            },
        })
    }

    /// `notifications/resources/updated` for `uri`, tagged with the
    /// stream's id when it is sent on one.
    pub(super) fn resource_updated(uri: &str, stream: Option<&Value>) -> Self {
        Notification::ResourceUpdated(ResourceUpdated {
            uri: uri.to_string(),
            meta: stream.map(|id| StreamMeta {
                subscription_id: id.clone(),
            }),
        })
    }

    /// Its JSON-RPC method.
    pub fn method(&self) -> &'static str {
        match self {
            Notification::Acknowledged(_) => "notifications/subscriptions/acknowledged",
            Notification::ResourceUpdated(_) => "notifications/resources/updated",
            Notification::GraphChanged(_) => "specforge/graphChanged",
            Notification::DiagnosticsChanged(_) => "specforge/diagnosticsChanged",
        }
    }

    /// The JSON-RPC message the transport writes:
    /// `{"jsonrpc":"2.0", "method", "params"}`.
    pub fn framed(&self) -> Value {
        #[derive(Serialize)]
        struct Framed<'a> {
            jsonrpc: &'static str,
            #[serde(flatten)]
            notification: &'a Notification,
        }
        serde_json::to_value(Framed {
            jsonrpc: "2.0",
            notification: self,
        })
        .expect("a notification serializes")
    }
}

/// `notifications/resources/updated`'s params.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResourceUpdated {
    pub uri: String,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<StreamMeta>,
}

/// `notifications/subscriptions/acknowledged`'s params.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Acknowledged {
    #[serde(rename = "_meta")]
    pub meta: StreamMeta,
    pub notifications: Honoured,
}

/// The `_meta` tying a notification to its `subscriptions/listen`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StreamMeta {
    /// The listen request's JSON-RPC id, a string or a number.
    #[serde(rename = "io.modelcontextprotocol/subscriptionId")]
    pub subscription_id: Value,
}

/// What a `subscriptions/listen` acknowledgement says was honoured.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Honoured {
    pub resource_subscriptions: Vec<String>,
}
