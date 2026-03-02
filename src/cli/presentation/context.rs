//! Context get presentation: text and json formatters.

use crate::api::NodeContext;
use crate::error::ApiError;
use crate::metadata::frame_types::project_visible_metadata;
use serde_json::json;

pub fn format_context_text_output(
    context: &NodeContext,
    include_metadata: bool,
    combine: bool,
    separator: &str,
    include_deleted: bool,
) -> Result<String, ApiError> {
    let frames: Vec<&crate::context::frame::Frame> = if include_deleted {
        context.frames.iter().collect()
    } else {
        context
            .frames
            .iter()
            .filter(|f| !f.is_deleted())
            .collect()
    };

    if frames.is_empty() {
        return Ok(format!(
            "Node: {}\nPath: {}\nNo frames found.",
            hex::encode(context.node_id),
            context.node_record.path.display()
        ));
    }

    if combine {
        let texts: Vec<String> = frames
            .iter()
            .filter_map(|f| f.text_content().ok())
            .collect();
        Ok(texts.join(separator))
    } else {
        let mut output = format!(
            "Node: {}\nPath: {}\nFrames: {}/{}\n\n",
            hex::encode(context.node_id),
            context.node_record.path.display(),
            frames.len(),
            context.frame_count
        );
        for (i, frame) in frames.iter().enumerate() {
            output.push_str(&format!("--- Frame {} ---\n", i + 1));
            if include_metadata {
                output.push_str(&format!("Frame ID: {}\n", hex::encode(frame.frame_id)));
                output.push_str(&format!("Frame Type: {}\n", frame.frame_type));
                if let Some(agent_id) = frame.agent_id() {
                    output.push_str(&format!("Agent: {}\n", agent_id));
                }
                output.push_str(&format!("Timestamp: {:?}\n", frame.timestamp));
                if !frame.metadata.is_empty() {
                    let projected = project_visible_metadata(&frame.metadata);
                    output.push_str("Metadata:\n");
                    for (key, value) in projected {
                        output.push_str(&format!("  {}: {}\n", key, value));
                    }
                }
                output.push_str("\n");
            }
            if let Ok(text) = frame.text_content() {
                output.push_str(&format!("Content:\n{}\n", text));
            } else {
                output.push_str("Content: [Binary content - not UTF-8]\n");
            }
            output.push_str("\n");
        }
        Ok(output)
    }
}

pub fn format_context_json_output(
    context: &NodeContext,
    include_metadata: bool,
    include_deleted: bool,
) -> Result<String, ApiError> {
    let frames: Vec<&crate::context::frame::Frame> = if include_deleted {
        context.frames.iter().collect()
    } else {
        context
            .frames
            .iter()
            .filter(|f| !f.is_deleted())
            .collect()
    };

    let frames_json: Vec<serde_json::Value> = frames
        .iter()
        .map(|frame| {
            let mut frame_obj = json!({
                "frame_id": hex::encode(frame.frame_id),
                "frame_type": frame.frame_type,
                "timestamp": frame.timestamp,
            });
            if include_metadata {
                if let Some(agent_id) = frame.agent_id() {
                    frame_obj["agent_id"] = json!(agent_id);
                }
                frame_obj["metadata"] = json!(project_visible_metadata(&frame.metadata));
            }
            if let Ok(text) = frame.text_content() {
                frame_obj["content"] = json!(text);
            } else {
                frame_obj["content"] = json!(null);
                frame_obj["content_binary"] = json!(true);
            }
            frame_obj
        })
        .collect();

    let result = json!({
        "node_id": hex::encode(context.node_id),
        "path": context.node_record.path.to_string_lossy(),
        "node_type": match context.node_record.node_type {
            crate::store::NodeType::File { size, .. } => format!("file:{}", size),
            crate::store::NodeType::Directory => "directory".to_string(),
        },
        "frames": frames_json,
        "frame_count": frames.len(),
        "total_frame_count": context.frame_count,
    });

    serde_json::to_string_pretty(&result)
        .map_err(|e| ApiError::ConfigError(format!("Failed to serialize JSON: {}", e)))
}
