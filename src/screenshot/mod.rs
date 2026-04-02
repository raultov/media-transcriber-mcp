use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rust_mcp_sdk::macros;
use rust_mcp_sdk::schema::CallToolError;
use rust_mcp_sdk::schema::{CallToolRequestParams, CallToolResult, ContentBlock, ImageContent};
use std::process::{Command, Stdio};
use tempfile::Builder;

use crate::sources::instagram::{download_instagram_video, is_instagram_url};
use crate::sources::reddit::{download_reddit_video, is_reddit_url};
use crate::sources::twitter::{download_twitter_video, is_twitter_url};
use crate::sources::vimeo::{download_vimeo_video, is_vimeo_url};
use crate::sources::youtube::{download_youtube_video, is_youtube_query};

#[macros::mcp_tool(
    name = "capture_screenshot",
    description = "Captures a high-quality screenshot from a video at a specific timestamp. USE CASE: Once you have the 'srt' transcription and identified a key moment (like a specific slide, diagram, or code block), use this tool to 'see' the detail. It is the final step for deep technical verification. Supports local files, YouTube URLs/queries, Vimeo URLs, Reddit URLs, Twitter/X URLs, and Instagram URLs. AUTHENTICATION: For sites like Instagram or private videos, pass 'browser_cookies' (e.g., 'chrome', 'firefox')."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct CaptureScreenshotTool {
    /// Local path to the video file, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, or an Instagram URL
    pub video_path: String,
    /// Timestamp to capture the screenshot at, e.g., "00:01:23" or "123" (seconds)
    pub timestamp: String,
    /// Optional source hint: 'youtube', 'vimeo', 'reddit', 'twitter', or 'instagram'. Auto-detected from the URL when omitted.
    pub source: Option<String>,
    /// Optional: Extract cookies from a browser for authentication (e.g., 'chrome', 'firefox', 'safari', 'edge'). Required for Instagram.
    pub browser_cookies: Option<String>,
}

#[macros::mcp_tool(
    name = "sample_video_scenes",
    description = "Extracts multiple keyframes automatically based on visual scene changes. USE CASE: For technical talks, tutorials, or guides, use this IMMEDIATELY alongside 'transcribe_media' (text) to get a visual storyboard. This helps you understand the context of slides or demos without reading the whole video. Supports local files, YouTube URLs/queries, Vimeo URLs, Reddit URLs, Twitter/X URLs, and Instagram URLs. AUTHENTICATION: For sites like Instagram or private videos, pass 'browser_cookies' (e.g., 'chrome', 'firefox')."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct SampleVideoScenesTool {
    /// Local path to the video file, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, or an Instagram URL
    pub video_path: String,
    /// Optional: Maximum number of frames to extract (default is 10)
    pub max_frames: Option<i32>,
    /// Optional source hint: 'youtube', 'vimeo', 'reddit', 'twitter', or 'instagram'. Auto-detected from the URL when omitted.
    pub source: Option<String>,
    /// Optional: Extract cookies from a browser for authentication (e.g., 'chrome', 'firefox', 'safari', 'edge'). Required for Instagram.
    pub browser_cookies: Option<String>,
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

pub fn sample_scenes(video_path: &str, max_frames: i32) -> Result<Vec<Vec<u8>>> {
    let temp_dir = tempfile::tempdir()?;
    let output_pattern = temp_dir.path().join("frame_%03d.jpg");

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            video_path,
            "-vf",
            "select='gt(scene,0.4)',scale='min(800,iw)':-1",
            "-fps_mode",
            "vfr",
            "-q:v",
            "2",
            "-frames:v",
            &max_frames.to_string(),
            output_pattern.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "ffmpeg failed to extract scenes. Make sure ffmpeg is installed and the video path is valid."
        ));
    }

    let mut images = Vec::new();
    let mut paths: Vec<_> = std::fs::read_dir(temp_dir.path())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jpg"))
        .collect();

    paths.sort();

    for path in paths {
        images.push(std::fs::read(path)?);
    }

    Ok(images)
}

/// Resolve the video path: download from YouTube/Vimeo when necessary, or use
/// the local path as-is.
fn resolve_video_path(
    video_path: &str,
    source_hint: Option<&str>,
    browser_cookies: Option<&str>,
) -> core::result::Result<String, CallToolError> {
    // Explicit source hint takes priority; otherwise auto-detect from URL.
    let use_instagram = source_hint
        .map(|s| s.eq_ignore_ascii_case("instagram"))
        .unwrap_or_else(|| is_instagram_url(video_path));

    let use_twitter = !use_instagram
        && source_hint
            .map(|s| s.eq_ignore_ascii_case("twitter"))
            .unwrap_or_else(|| is_twitter_url(video_path));

    let use_reddit = !use_instagram
        && !use_twitter
        && source_hint
            .map(|s| s.eq_ignore_ascii_case("reddit"))
            .unwrap_or_else(|| is_reddit_url(video_path));

    let use_vimeo = !use_instagram
        && !use_twitter
        && !use_reddit
        && source_hint
            .map(|s| s.eq_ignore_ascii_case("vimeo"))
            .unwrap_or_else(|| is_vimeo_url(video_path));

    let use_youtube = !use_instagram
        && !use_twitter
        && !use_reddit
        && !use_vimeo
        && source_hint
            .map(|s| s.eq_ignore_ascii_case("youtube"))
            .unwrap_or_else(|| is_youtube_query(video_path));

    if use_instagram {
        download_instagram_video(video_path, browser_cookies)
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| CallToolError::from_message(e.to_string()))
    } else if use_twitter {
        download_twitter_video(video_path, browser_cookies)
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| CallToolError::from_message(e.to_string()))
    } else if use_reddit {
        download_reddit_video(video_path, browser_cookies)
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| CallToolError::from_message(e.to_string()))
    } else if use_vimeo {
        download_vimeo_video(video_path, browser_cookies)
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| CallToolError::from_message(e.to_string()))
    } else if use_youtube {
        download_youtube_video(video_path, browser_cookies)
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| CallToolError::from_message(e.to_string()))
    } else {
        Ok(video_path.to_string())
    }
}

pub async fn handle_capture_screenshot(
    params: CallToolRequestParams,
) -> core::result::Result<CallToolResult, CallToolError> {
    let args_value: serde_json::Value = params.arguments.unwrap_or_default().into();
    let args: CaptureScreenshotTool = serde_json::from_value(args_value)
        .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))?;

    let final_video_path = resolve_video_path(
        &args.video_path,
        args.source.as_deref(),
        args.browser_cookies.as_deref(),
    )?;

    match extract_screenshot(&final_video_path, &args.timestamp) {
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

pub async fn handle_sample_scenes(
    params: CallToolRequestParams,
) -> core::result::Result<CallToolResult, CallToolError> {
    let args_value: serde_json::Value = params.arguments.unwrap_or_default().into();
    let args: SampleVideoScenesTool = serde_json::from_value(args_value)
        .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))?;

    let final_video_path = resolve_video_path(
        &args.video_path,
        args.source.as_deref(),
        args.browser_cookies.as_deref(),
    )?;

    let max_frames = args.max_frames.unwrap_or(10);

    match sample_scenes(&final_video_path, max_frames) {
        Ok(images) => {
            if images.is_empty() {
                return Ok(CallToolResult::text_content(vec![
                    "No scene changes detected or video is too short.".into(),
                ]));
            }

            let content: Vec<ContentBlock> = images
                .into_iter()
                .map(|img| {
                    ContentBlock::ImageContent(ImageContent::new(
                        STANDARD.encode(img),
                        "image/jpeg".to_string(),
                        None,
                        None,
                    ))
                })
                .collect();

            Ok(CallToolResult {
                content,
                is_error: Some(false),
                meta: None,
                structured_content: None,
            })
        }
        Err(e) => Err(CallToolError::from_message(format!(
            "Scene extraction error: {}",
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

    #[test]
    fn test_sample_scenes_invalid_file() {
        let result = sample_scenes("non_existent_file.mp4", 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_sample_scenes_valid_video() {
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
            let result = sample_scenes(dummy_video.path().to_str().unwrap(), 2);
            assert!(result.is_ok());
        }
    }
}
