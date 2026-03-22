use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rust_mcp_sdk::macros;
use rust_mcp_sdk::schema::CallToolError;
use rust_mcp_sdk::schema::{CallToolRequestParams, CallToolResult, ContentBlock, ImageContent};
use std::process::{Command, Stdio};
use tempfile::Builder;

#[macros::mcp_tool(
    name = "capture_screenshot",
    description = "Captures a screenshot from a video file at a specific timestamp. The LLM can use this to get visual context like graphs, slides, schemas in a technical talk."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct CaptureScreenshotTool {
    /// Local path to the video file
    pub video_path: String,
    /// Timestamp to capture the screenshot at, e.g., "00:01:23" or "123" (seconds)
    pub timestamp: String,
}

pub fn extract_screenshot(video_path: &str, timestamp: &str) -> Result<Vec<u8>> {
    let temp_file = Builder::new().suffix(".jpg").tempfile()?;
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            timestamp,
            "-i",
            video_path,
            "-vframes",
            "1",
            "-q:v",
            "2",
            "-f",
            "image2",
            temp_file.path().to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "ffmpeg failed to extract screenshot. Make sure ffmpeg is installed and the video path/timestamp are valid."
        ));
    }

    let image_data = std::fs::read(temp_file.path())?;
    Ok(image_data)
}

pub async fn handle_capture_screenshot(
    params: CallToolRequestParams,
) -> core::result::Result<CallToolResult, CallToolError> {
    let args_value: serde_json::Value = params.arguments.unwrap_or_default().into();
    let args: CaptureScreenshotTool = serde_json::from_value(args_value)
        .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))?;

    match extract_screenshot(&args.video_path, &args.timestamp) {
        Ok(image_data) => {
            let base64_image = STANDARD.encode(image_data);
            Ok(CallToolResult {
                content: vec![ContentBlock::ImageContent(ImageContent::new(
                    base64_image,
                    "image/jpeg".to_string(),
                    None,
                    None,
                ))],
                is_error: Some(false),
                meta: None,
                structured_content: None,
            })
        }
        Err(e) => Err(CallToolError::from_message(format!(
            "Screenshot extraction error: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn test_extract_screenshot_invalid_file() {
        let result = extract_screenshot("non_existent_file.mp4", "00:00:01");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_screenshot_valid_video() {
        // Create a dummy video file using ffmpeg
        let dummy_video = tempfile::Builder::new().suffix(".mp4").tempfile().unwrap();
        let status = Command::new("ffmpeg")
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=128x128:rate=10",
                "-y",
                dummy_video.path().to_str().unwrap(),
            ])
            .output()
            .expect("Failed to start ffmpeg for test setup");

        if status.status.success() {
            let result = extract_screenshot(dummy_video.path().to_str().unwrap(), "00:00:01");
            assert!(result.is_ok());
            let image_data = result.unwrap();
            assert!(image_data.len() > 0);
        }
    }
}
