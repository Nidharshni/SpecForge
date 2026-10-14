use super::ModelIntermediate;

pub(super) fn render_json(model: &ModelIntermediate) -> String {
    serde_json::to_string_pretty(model).expect("model serialization cannot fail")
}
