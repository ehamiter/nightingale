use iced::{
    Element, Task,
    widget::{button, column, container, image, row, scrollable, text, text_input, Image},
    Length, Subscription,
    keyboard,
    event,
};
use iced::futures::Stream;
use iced::widget::text_input::Id as TextInputId;
use iced::widget::scrollable::Id as ScrollableId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio::sync::mpsc;

// Config for persistent settings
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    download_directory: Option<PathBuf>,
    browser_for_cookies: Option<String>, // chrome, firefox, safari, etc.
}

impl Default for Config {
    fn default() -> Self {
        Self {
            download_directory: None,
            browser_for_cookies: Some("safari".to_string()), // Default to Safari on macOS
        }
    }
}

impl Config {
    fn load() -> Self {
        if let Some(config_dir) = dirs::config_dir() {
            let config_file = config_dir.join("nightingale").join("config.json");
            if let Ok(contents) = std::fs::read_to_string(&config_file) {
                if let Ok(config) = serde_json::from_str(&contents) {
                    return config;
                }
            }
        }
        Self::default()
    }
    
    fn save(&self) -> Result<(), String> {
        if let Some(config_dir) = dirs::config_dir() {
            let nightingale_config_dir = config_dir.join("nightingale");
            std::fs::create_dir_all(&nightingale_config_dir)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
            
            let config_file = nightingale_config_dir.join("config.json");
            let contents = serde_json::to_string_pretty(self)
                .map_err(|e| format!("Failed to serialize config: {}", e))?;
            
            std::fs::write(&config_file, contents)
                .map_err(|e| format!("Failed to write config: {}", e))?;
        }
        Ok(())
    }
}

// Helper function to get yt-dlp binary path in local directory
fn get_ytdlp_path() -> PathBuf {
    let home = dirs::home_dir().expect("Could not find home directory");
    home.join(".local").join("bin").join("yt-dlp")
}

// Check if yt-dlp is installed and executable
fn is_ytdlp_installed() -> bool {
    let path = get_ytdlp_path();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.exists() && path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.exists()
    }
}

// Fix yt-dlp shebang to use Python from PATH
fn fix_ytdlp_shebang() -> Result<(), String> {
    let ytdlp_path = get_ytdlp_path();
    
    if !ytdlp_path.exists() {
        return Ok(());
    }
    
    #[cfg(unix)]
    {
        // Find python3 in PATH
        let python_path = std::process::Command::new("which")
            .arg("python3")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "/usr/bin/env python3".to_string());
        
        let contents = std::fs::read_to_string(&ytdlp_path)
            .map_err(|e| format!("Failed to read yt-dlp: {}", e))?;
        
        // Replace any python shebang with the one from PATH
        if contents.starts_with("#!/") {
            if let Some(newline_pos) = contents.find('\n') {
                let shebang = format!("#!{}\n", python_path);
                let fixed = format!("{}{}", shebang, &contents[newline_pos + 1..]);
                std::fs::write(&ytdlp_path, fixed)
                    .map_err(|e| format!("Failed to write fixed yt-dlp: {}", e))?;
            }
        }
    }
    
    Ok(())
}

// Download yt-dlp nightly binary for the current platform
async fn download_ytdlp() -> Result<(), String> {
    let ytdlp_path = get_ytdlp_path();
    
    // Create .local/bin directory if it doesn't exist
    if let Some(parent) = ytdlp_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    
    // Determine platform-specific download URL
    let download_url = if cfg!(target_os = "macos") {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
    } else if cfg!(target_os = "linux") {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
    } else {
        return Err("Unsupported platform".to_string());
    };
    
    // Download the binary
    let client = reqwest::Client::new();
    let response = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download yt-dlp: {}", e))?;
    
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {}", e))?;
    
    // Write to file
    std::fs::write(&ytdlp_path, bytes)
        .map_err(|e| format!("Failed to write yt-dlp binary: {}", e))?;
    
    // Make executable (Unix-like systems)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&ytdlp_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&ytdlp_path, perms)
            .map_err(|e| format!("Failed to set executable permissions: {}", e))?;
    }
    
    fix_ytdlp_shebang()?;
    
    Ok(())
}

// Helper function to find yt-dlp binary
fn find_ytdlp() -> String {
    let local_path = get_ytdlp_path();
    if local_path.exists() {
        return local_path.to_string_lossy().to_string();
    }
    
    // Fallback to system installations
    let possible_paths = vec![
        "/opt/homebrew/bin/yt-dlp",      // Homebrew (Apple Silicon)
        "/usr/local/bin/yt-dlp",          // Homebrew (Intel)
        "/usr/bin/yt-dlp",                // System install
        "yt-dlp",                         // In PATH
    ];
    
    for path in possible_paths {
        if std::path::Path::new(path).exists() || path == "yt-dlp" {
            return path.to_string();
        }
    }
    
    // Default to local path (even if not exists, will trigger error message)
    local_path.to_string_lossy().to_string()
}

// Helper function to find ffmpeg location
fn find_ffmpeg() -> Option<String> {
    let possible_paths = vec![
        "/opt/homebrew/bin/ffmpeg",      // Homebrew (Apple Silicon)
        "/usr/local/bin/ffmpeg",          // Homebrew (Intel)
        "/usr/bin/ffmpeg",                // System install
    ];
    
    for path in possible_paths {
        if std::path::Path::new(path).exists() {
            // Return the directory containing ffmpeg
            return Some(std::path::Path::new(path).parent()?.to_str()?.to_string());
        }
    }
    
    None
}

async fn load_thumbnail(url: &str) -> Result<image::Handle, String> {
    let bytes = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to download thumbnail: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read thumbnail bytes: {}", e))?;
    
    Ok(image::Handle::from_bytes(bytes))
}

fn clean_filename(title: &str) -> String {
    let mut cleaned = title.to_string();
    
    // Remove common patterns like (Official Music Video), [Official Video], etc.
    let patterns = [
        " (Official Music Video)",
        " (Official Video)",
        " [Official Music Video]",
        " [Official Video]",
        " (Official Audio)",
        " [Official Audio]",
        " (Lyric Video)",
        " [Lyric Video]",
        " (Lyrics)",
        " [Lyrics]",
        " (Music Video)",
        " [Music Video]",
        " (HD)",
        " [HD]",
        " (4K)",
        " [4K]",
    ];
    
    for pattern in &patterns {
        cleaned = cleaned.replace(pattern, "");
    }
    
    cleaned.trim().to_string()
}

// Message enum for download updates
#[derive(Debug, Clone)]
enum DownloadUpdate {
    Progress(f32),
    Log(String),
    Completed(Result<String, String>),
}

fn download_mp3_stream_with_filename(video_id: String, download_dir: PathBuf, filename: String) -> impl Stream<Item = DownloadUpdate> {
    let (tx, rx) = mpsc::unbounded_channel();
    
    tokio::spawn(async move {
        let url = format!("https://www.youtube.com/watch?v={}", video_id);
        
        let output_template = download_dir
            .join(format!("{}.%(ext)s", filename))
            .to_string_lossy()
            .to_string();
        
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;
        
        let ytdlp_path = find_ytdlp();
        
        let result = async {
            let mut cmd = Command::new(&ytdlp_path);
            cmd.arg("-x")
                .arg("--audio-format")
                .arg("mp3")
                .arg("--no-playlist")
                .arg("--verbose");
            
            if let Some(ffmpeg_dir) = find_ffmpeg() {
                cmd.arg("--ffmpeg-location").arg(&ffmpeg_dir);
            }
            
            cmd.arg("--extractor-retries")
                .arg("5")
                .arg("--fragment-retries")
                .arg("5")
                .arg("--newline")
                .arg("--progress-template")
                .arg("download:%(progress.downloaded_bytes)s/%(progress.total_bytes)s")
                .arg("-o")
                .arg(&output_template)
                .arg(&url)
                .current_dir(&download_dir)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            
            let mut child = cmd.spawn()
                .map_err(|e| format!("Failed to run yt-dlp (is it installed?): {}", e))?;
            
            let _ = tx.send(DownloadUpdate::Progress(0.0));
            
            let stdout_handle = child.stdout.take();
            let stderr_handle = child.stderr.take();
            
            let tx_stderr = tx.clone();
            if let Some(stderr) = stderr_handle {
                tokio::spawn(async move {
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let _ = tx_stderr.send(DownloadUpdate::Log(line));
                    }
                });
            }
            
            if let Some(stdout) = stdout_handle {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx.send(DownloadUpdate::Log(line.clone()));
                    
                    if line.starts_with("download:") {
                        if let Some(progress_part) = line.strip_prefix("download:") {
                            let parts: Vec<&str> = progress_part.split('/').collect();
                            if parts.len() == 2 {
                                if let (Ok(downloaded), Ok(total)) = (
                                    parts[0].parse::<f32>(),
                                    parts[1].parse::<f32>(),
                                ) {
                                    if total > 0.0 {
                                        let percent = (downloaded / total * 100.0).min(100.0);
                                        let _ = tx.send(DownloadUpdate::Progress(percent));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            let output = child.wait().await
                .map_err(|e| format!("Failed to wait for yt-dlp: {}", e))?;
            
            if !output.success() {
                let error_msg = format!("yt-dlp failed with exit code: {:?}. Check logs for details.", output.code());
                return Err(error_msg);
            }
            
            Ok(format!("Downloaded successfully to {}", download_dir.display()))
        }.await;
        
        let _ = tx.send(DownloadUpdate::Completed(result));
    });
    
    UnboundedReceiverStream::new(rx)
}

// Check if input is a YouTube URL
fn is_youtube_url(input: &str) -> bool {
    input.contains("youtube.com/") || input.contains("youtu.be/")
}



// Get video info from URL using yt-dlp
async fn get_video_info_from_url(url: &str) -> Result<Vec<VideoResult>, String> {
    use tokio::process::Command;
    
    let ytdlp_path = find_ytdlp();
    
    let output = Command::new(&ytdlp_path)
        .arg("--dump-json")
        .arg("--flat-playlist")
        .arg(url)
        .output()
        .await
        .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to get video info: {}", stderr));
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();
    
    // Each line is a JSON object for playlist entries
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        
        let json: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| format!("Failed to parse video info: {}", e))?;
        
        let video_id = json["id"].as_str().unwrap_or("").to_string();
        let title = json["title"].as_str().unwrap_or("Unknown Title").to_string();
        let channel = json["uploader"].as_str()
            .or_else(|| json["channel"].as_str())
            .unwrap_or("Unknown Channel").to_string();
        
        let duration_secs = json["duration"].as_f64().unwrap_or(0.0) as u64;
        let duration = if duration_secs > 0 {
            format!("{}:{:02}", duration_secs / 60, duration_secs % 60)
        } else {
            "Unknown".to_string()
        };
        
        let view_count = json["view_count"].as_u64();
        let views = if let Some(count) = view_count {
            if count >= 1_000_000 {
                format!("{:.1}M views", count as f64 / 1_000_000.0)
            } else if count >= 1_000 {
                format!("{:.1}K views", count as f64 / 1_000.0)
            } else {
                format!("{} views", count)
            }
        } else {
            "Unknown views".to_string()
        };
        
        let thumbnail = json["thumbnail"].as_str()
            .or_else(|| json["thumbnails"].as_array()
                .and_then(|t| t.first())
                .and_then(|t| t["url"].as_str()))
            .unwrap_or("").to_string();
        
        if !video_id.is_empty() {
            results.push(VideoResult {
                title,
                video_id,
                channel,
                duration,
                views,
                thumbnail,
            });
        }
    }
    
    if results.is_empty() {
        Err("No videos found in URL".to_string())
    } else {
        Ok(results)
    }
}

async fn search_youtube(query: &str) -> Result<Vec<VideoResult>, String> {
    // Check if input is a YouTube URL
    if is_youtube_url(query) {
        return get_video_info_from_url(query).await;
    }
    
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let url = format!("https://www.youtube.com/results?search_query={}", 
        urlencoding::encode(query));
    
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let html = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Extract JSON data from the page
    let json_start = html.find("var ytInitialData = ")
        .ok_or_else(|| "Could not find video data in page".to_string())?;
    
    let json_start = json_start + "var ytInitialData = ".len();
    let json_end = html[json_start..]
        .find(";</script>")
        .ok_or_else(|| "Could not parse video data".to_string())?;
    
    let json_str = &html[json_start..json_start + json_end];
    let json: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    // Parse video results from JSON
    let mut results = Vec::new();
    
    if let Some(contents) = json["contents"]["twoColumnSearchResultsRenderer"]
        ["primaryContents"]["sectionListRenderer"]["contents"][0]
        ["itemSectionRenderer"]["contents"].as_array() {
        
        for item in contents {
            if let Some(video) = item.get("videoRenderer") {
                let title = video["title"]["runs"][0]["text"]
                    .as_str()
                    .unwrap_or("Unknown Title")
                    .to_string();
                
                let video_id = video["videoId"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                
                let channel = video["ownerText"]["runs"][0]["text"]
                    .as_str()
                    .unwrap_or("Unknown Channel")
                    .to_string();
                
                let duration = video["lengthText"]["simpleText"]
                    .as_str()
                    .unwrap_or("Unknown")
                    .to_string();
                
                let views = video["viewCountText"]["simpleText"]
                    .as_str()
                    .or_else(|| video["shortViewCountText"]["simpleText"].as_str())
                    .unwrap_or("Unknown views")
                    .to_string();
                
                let thumbnail = video["thumbnail"]["thumbnails"][0]["url"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                
                if !video_id.is_empty() {
                    results.push(VideoResult {
                        title,
                        video_id,
                        channel,
                        duration,
                        views,
                        thumbnail,
                    });
                }
            }
        }
    }

    if results.is_empty() {
        Err("No videos found".to_string())
    } else {
        // Sort results by relevance score
        results.sort_by(|a, b| {
            let score_a = a.calculate_score(query);
            let score_b = b.calculate_score(query);
            score_b.cmp(&score_a) // Higher scores first
        });
        
        Ok(results)
    }
}

pub fn main() -> iced::Result {
    iced::application("Songbird - YouTube Search", Songbird::update, Songbird::view)
        .subscription(Songbird::subscription)
        .theme(|_| iced::Theme::TokyoNightStorm)
        .run_with(Songbird::new)
}

#[derive(Debug, Clone, Deserialize)]
struct VideoResult {
    title: String,
    video_id: String,
    channel: String,
    duration: String,
    views: String,
    thumbnail: String,
}

impl VideoResult {
    fn url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.video_id)
    }
    
    fn calculate_score(&self, search_query: &str) -> i32 {
        let mut score = 0;
        
        let title_lower = self.title.to_lowercase();
        let query_lower = search_query.to_lowercase();
        
        // Explicit versions are preferred over censored
        if title_lower.contains("explicit") {
            score += 40;
        }
        
        // Official/Original audio should always be at the top
        if title_lower.contains("official audio") || title_lower.contains("original audio") {
            score += 200;
        }
        
        // Audio versions are preferred (clean audio, no video interruptions)
        if title_lower.contains("(audio)") || title_lower.contains("[audio]") {
            score += 50;
        }
        
        if title_lower.contains("audio") && !title_lower.contains("audiobook") {
            score += 25;
        }
        
        // Official indicators (but not if it's a video)
        if title_lower.contains("official") && !title_lower.contains("video") {
            score += 30;
        }
        
        if (title_lower.contains("(official") || title_lower.contains("[official")) && !title_lower.contains("video") {
            score += 20;
        }
        
        // Heavily penalize any video unless explicitly searched for
        if !query_lower.contains("video") {
            if title_lower.contains("video") {
                score -= 150;
            }
            if title_lower.contains("music video") {
                score -= 50;
            }
            if title_lower.contains("official video") || title_lower.contains("official music video") {
                score -= 75;
            }
        }
        
        // Parse view count for popularity bonus
        if let Some(view_score) = self.parse_view_score() {
            score += view_score;
        }
        
        // Penalize covers, remixes, extended versions (unless explicitly searched for)
        if !query_lower.contains("cover") && (title_lower.contains("cover") || title_lower.contains("covered by")) {
            score -= 30;
        }
        
        if !query_lower.contains("remix") && title_lower.contains("remix") {
            score -= 35;
        }
        
        if !query_lower.contains("extended") && title_lower.contains("extended") {
            score -= 30;
        }
        
        // Penalize lyric videos (lower quality)
        if title_lower.contains("lyric") && !title_lower.contains("official") {
            score -= 15;
        }
        
        score
    }
    
    fn parse_view_score(&self) -> Option<i32> {
        let views = self.views.to_lowercase();
        
        // Extract number from strings like "1.2M views" or "1,234,567 views"
        if views.contains('m') {
            // Millions of views
            if let Some(num_str) = views.split_whitespace().next() {
                if let Ok(num) = num_str.replace('m', "").replace(',', "").parse::<f32>() {
                    return Some((num * 10.0).min(50.0) as i32); // Cap at +50
                }
            }
        } else if views.contains('k') {
            // Thousands of views
            if let Some(num_str) = views.split_whitespace().next() {
                if let Ok(num) = num_str.replace('k', "").replace(',', "").parse::<f32>() {
                    return Some((num / 100.0).min(30.0) as i32); // Cap at +30
                }
            }
        } else {
            // Raw number with commas
            if let Some(num_str) = views.split_whitespace().next() {
                if let Ok(num) = num_str.replace(',', "").parse::<i32>() {
                    return Some((num / 1_000_000).min(50) as i32); // Cap at +50
                }
            }
        }
        
        None
    }
}

#[derive(Debug, Clone)]
enum Message {
    SearchInputChanged(String),
    SearchPressed,
    SearchCompleted(Result<Vec<VideoResult>, String>),
    ThumbnailLoaded(String, Result<image::Handle, String>),
    DownloadMp3(String), // video_id
    DownloadProgress(String, f32), // video_id, progress (0-100)
    DownloadLog(String, String), // video_id, log line
    DownloadCompleted(String, Result<String, String>), // video_id, result message
    OpenUrl(String), // url to open in browser
    ToggleSettings, // Open/close settings view
    PickDirectory, // Open native directory picker
    DirectoryPicked(Option<PathBuf>), // Result from directory picker
    ShowLogs(String), // video_id
    CopyLogs(String), // video_id
    CloseLogs,
    ShowPlayerLogs,
    CopyPlayerLogs,
    ClosePlayerLogs,
    KeyboardEvent(keyboard::Event), // Keyboard events
    InstallYtDlp, // Install yt-dlp binary
    YtDlpInstalled(Result<(), String>), // Result of installation
    ShowRenameModal(String), // video_id
    RenameFilenameChanged(String),
    ConfirmDownload,
    CancelRename,
}

struct Songbird {
    search_query: String,
    search_results: Vec<VideoResult>,
    is_searching: bool,
    error_message: Option<String>,
    thumbnails: HashMap<String, image::Handle>,
    downloading: HashMap<String, bool>, // video_id -> is_downloading
    download_messages: HashMap<String, String>, // video_id -> status message
    download_progress: HashMap<String, f32>, // video_id -> progress (0-100)
    download_logs: HashMap<String, Vec<String>>, // video_id -> log lines
    config: Config,
    show_settings: bool,
    show_logs_for: Option<String>, // video_id to show logs for
    search_input_id: TextInputId,
    results_scroll_id: ScrollableId,
    ytdlp_status: String, // Status message for yt-dlp installation/update
    ytdlp_installing: bool,
    player_logs: Vec<String>,
    show_player_logs: bool,
    rename_modal: Option<RenameModal>,
}

struct RenameModal {
    video_id: String,
    filename: String,
}

impl Songbird {
    fn new() -> (Self, Task<Message>) {
        let ytdlp_status = if is_ytdlp_installed() {
            "yt-dlp is installed".to_string()
        } else {
            "yt-dlp not found - click Install to download".to_string()
        };
        
        let search_input_id = TextInputId::unique();
        let results_scroll_id = ScrollableId::unique();
        let focus_task = text_input::focus(search_input_id.clone());
        
        let app = Self {
            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            error_message: None,
            thumbnails: HashMap::new(),
            downloading: HashMap::new(),
            download_messages: HashMap::new(),
            download_progress: HashMap::new(),
            download_logs: HashMap::new(),
            config: Config::load(),
            show_settings: false,
            show_logs_for: None,
            search_input_id,
            results_scroll_id,
            ytdlp_status,
            ytdlp_installing: false,
            player_logs: Vec::new(),
            show_player_logs: false,
            rename_modal: None,
        };
        
        (app, focus_task)
    }
}

impl Songbird {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SearchInputChanged(value) => {
                self.search_query = value;
                self.error_message = None;
                Task::none()
            }
            Message::SearchPressed => {
                if self.search_query.trim().is_empty() {
                    self.error_message = Some("Please enter a search query".to_string());
                    return Task::none();
                }

                self.is_searching = true;
                self.error_message = None;
                let query = self.search_query.clone();

                Task::perform(
                    async move {
                        search_youtube(&query).await
                    },
                    Message::SearchCompleted,
                )
            }
            Message::SearchCompleted(result) => {
                self.is_searching = false;
                match result {
                    Ok(results) => {
                        self.search_results = results;
                        if self.search_results.is_empty() {
                            self.error_message = Some("No results found".to_string());
                            return Task::none();
                        }
                        
                        // Load thumbnails for all results
                        let thumbnail_tasks: Vec<_> = self.search_results
                            .iter()
                            .map(|video| {
                                let url = video.thumbnail.clone();
                                let video_id = video.video_id.clone();
                                Task::perform(
                                    async move {
                                        match load_thumbnail(&url).await {
                                            Ok(handle) => (video_id, Ok(handle)),
                                            Err(e) => (video_id, Err(e)),
                                        }
                                    },
                                    |(video_id, result)| Message::ThumbnailLoaded(video_id, result),
                                )
                            })
                            .collect();
                        
                        return Task::batch(thumbnail_tasks);
                    }
                    Err(e) => {
                        self.error_message = Some(e);
                    }
                }
                Task::none()
            }
            Message::ThumbnailLoaded(video_id, result) => {
                if let Ok(handle) = result {
                    self.thumbnails.insert(video_id, handle);
                }
                Task::none()
            }
            Message::DownloadMp3(video_id) => {
                // Check if download directory is set
                if self.config.download_directory.is_none() {
                    self.show_settings = true;
                    self.error_message = Some("Please select a download directory in settings".to_string());
                    return Task::none();
                }
                
                // Show rename modal instead of downloading directly
                return self.update(Message::ShowRenameModal(video_id));
            }
            Message::ShowRenameModal(video_id) => {
                if let Some(video) = self.search_results.iter().find(|v| v.video_id == video_id) {
                    let filename = clean_filename(&video.title);
                    self.rename_modal = Some(RenameModal {
                        video_id: video_id.clone(),
                        filename,
                    });
                }
                Task::none()
            }
            Message::RenameFilenameChanged(new_name) => {
                if let Some(modal) = &mut self.rename_modal {
                    modal.filename = new_name;
                }
                Task::none()
            }
            Message::CancelRename => {
                self.rename_modal = None;
                Task::none()
            }
            Message::ConfirmDownload => {
                if let Some(modal) = self.rename_modal.take() {
                    let download_dir = self.config.download_directory.clone().unwrap();
                    let video_id = modal.video_id.clone();
                    let filename = modal.filename.clone();
                    
                    self.downloading.insert(video_id.clone(), true);
                    self.download_progress.insert(video_id.clone(), 0.0);
                    self.download_logs.insert(video_id.clone(), Vec::new());
                    self.download_messages.insert(video_id.clone(), "Starting download...".to_string());
                    
                    let vid_id = video_id.clone();
                    
                    // Use Task::run to stream progress updates!
                    Task::run(
                        download_mp3_stream_with_filename(video_id, download_dir, filename),
                        move |update| match update {
                            DownloadUpdate::Progress(percent) => {
                                Message::DownloadProgress(vid_id.clone(), percent)
                            }
                            DownloadUpdate::Log(log) => {
                                Message::DownloadLog(vid_id.clone(), log)
                            }
                            DownloadUpdate::Completed(result) => {
                                Message::DownloadCompleted(vid_id.clone(), result)
                            }
                        }
                    )
                } else {
                    Task::none()
                }
            }
            Message::DownloadProgress(video_id, progress) => {
                self.download_progress.insert(video_id, progress);
                Task::none()
            }
            Message::DownloadLog(video_id, log) => {
                self.download_logs.entry(video_id).or_insert_with(Vec::new).push(log);
                Task::none()
            }
            Message::DownloadCompleted(video_id, result) => {
                self.downloading.insert(video_id.clone(), false);
                self.download_progress.remove(&video_id);
                match result {
                    Ok(msg) => {
                        self.download_messages.insert(video_id, msg);
                    }
                    Err(e) => {
                        self.download_messages.insert(video_id, format!("Error: {}", e));
                    }
                }
                Task::none()
            }
            Message::OpenUrl(url) => {
                // Open URL in the default browser
                let _ = open::that(&url);
                Task::none()
            }
            Message::ToggleSettings => {
                self.show_settings = !self.show_settings;
                Task::none()
            }
            Message::PickDirectory => {
                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Select Download Directory")
                            .pick_folder()
                            .await
                            .map(|folder| folder.path().to_path_buf())
                    },
                    Message::DirectoryPicked,
                )
            }
            Message::DirectoryPicked(path) => {
                if let Some(dir) = path {
                    self.config.download_directory = Some(dir);
                    if let Err(e) = self.config.save() {
                        self.error_message = Some(format!("Failed to save config: {}", e));
                    } else {
                        self.error_message = None;
                    }
                }
                Task::none()
            }
            Message::ShowLogs(video_id) => {
                self.show_logs_for = Some(video_id);
                Task::none()
            }
            Message::CopyLogs(video_id) => {
                if let Some(logs) = self.download_logs.get(&video_id) {
                    let log_text = logs.join("\n");
                    #[cfg(target_os = "macos")]
                    {
                        use std::process::Command;
                        let mut child = Command::new("pbcopy")
                            .stdin(std::process::Stdio::piped())
                            .spawn()
                            .ok();
                        if let Some(ref mut child) = child {
                            use std::io::Write;
                            if let Some(ref mut stdin) = child.stdin {
                                let _ = stdin.write_all(log_text.as_bytes());
                            }
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        // For Linux, we'd use xclip or similar, but for now just show message
                        self.error_message = Some("Logs copied! (On Linux, please manually copy from the log viewer)".to_string());
                    }
                }
                Task::none()
            }
            Message::CloseLogs => {
                self.show_logs_for = None;
                Task::none()
            }
            Message::ShowPlayerLogs => {
                self.show_player_logs = true;
                Task::none()
            }
            Message::CopyPlayerLogs => {
                let log_text = self.player_logs.join("\n");
                #[cfg(target_os = "macos")]
                {
                    use std::process::Command;
                    let mut child = Command::new("pbcopy")
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                        .ok();
                    if let Some(ref mut child) = child {
                        use std::io::Write;
                        if let Some(ref mut stdin) = child.stdin {
                            let _ = stdin.write_all(log_text.as_bytes());
                        }
                    }
                }
                Task::none()
            }
            Message::ClosePlayerLogs => {
                self.show_player_logs = false;
                Task::none()
            }
            Message::KeyboardEvent(event) => {
                if let keyboard::Event::KeyPressed {
                    key: keyboard::Key::Character(c),
                    modifiers,
                    ..
                } = event
                {
                    if modifiers.command() && c.as_ref() == "k" {
                        return text_input::focus(self.search_input_id.clone());
                    }
                }
                Task::none()
            }
            Message::InstallYtDlp => {
                self.ytdlp_installing = true;
                self.ytdlp_status = "Downloading yt-dlp...".to_string();
                Task::perform(download_ytdlp(), Message::YtDlpInstalled)
            }
            Message::YtDlpInstalled(result) => {
                self.ytdlp_installing = false;
                match result {
                    Ok(()) => {
                        self.ytdlp_status = "yt-dlp installed successfully!".to_string();
                    }
                    Err(e) => {
                        self.ytdlp_status = format!("Installation failed: {}", e);
                    }
                }
                Task::none()
            }

        }
    }
    
    fn subscription(&self) -> Subscription<Message> {
        event::listen().map(|event| {
            if let event::Event::Keyboard(keyboard_event) = event {
                Message::KeyboardEvent(keyboard_event)
            } else {
                Message::KeyboardEvent(keyboard::Event::KeyReleased {
                    key: keyboard::Key::Character("".into()),
                    modifiers: keyboard::Modifiers::default(),
                    location: keyboard::Location::Standard,
                })
            }
        })
    }

    fn view(&self) -> Element<'_, Message> {
        if self.show_player_logs {
            return self.player_logs_view();
        }
        
        if let Some(video_id) = &self.show_logs_for {
            return self.logs_view(video_id);
        }
        
        if self.show_settings {
            return self.settings_view();
        }
        
        if let Some(modal) = &self.rename_modal {
            return self.rename_modal_view(modal);
        }
        
        let title = text("YouTube Video Search")
            .size(32)
            .width(Length::Fill);
        
        let settings_button = button(text("⚙").size(24))
            .on_press(Message::ToggleSettings)
            .padding(8);
        
        let title_row = row![title, settings_button]
            .spacing(10)
            .width(Length::Fill);

        let search_input = text_input("Enter search query...", &self.search_query)
            .on_input(Message::SearchInputChanged)
            .on_submit(Message::SearchPressed)
            .padding(10)
            .size(16)
            .width(Length::Fill)
            .id(self.search_input_id.clone());

        let search_button = button(
            text(if self.is_searching {
                "Searching..."
            } else {
                "Search"
            })
            .size(16),
        )
        .on_press_maybe(if self.is_searching {
            None
        } else {
            Some(Message::SearchPressed)
        })
        .padding(10);

        let search_row = row![search_input, search_button]
            .spacing(10)
            .width(Length::Fill);

        let mut header = column![title_row, search_row].spacing(20);

        // Show error message if any
        if let Some(error) = &self.error_message {
            header = header.push(
                text(error)
                    .size(14)
                    .style(|_theme| text::Style {
                        color: Some(iced::Color::from_rgb(0.8, 0.2, 0.2)),
                    }),
            );
        }

        // Show results
        let content = if !self.search_results.is_empty() {
            let results_title = text(format!("Results ({})", self.search_results.len()))
                .size(20)
                .width(Length::Fill);

            let mut results_list = column![].spacing(10);

            for video in &self.search_results {
                let video_title = text(&video.title)
                    .size(16)
                    .width(Length::Fill);

                let video_channel = text(format!("Channel: {}", video.channel))
                    .size(14)
                    .style(|_theme| text::Style {
                        color: Some(iced::Color::from_rgb(0.6, 0.6, 0.6)),
                    });

                let video_duration = text(format!("Duration: {}", video.duration))
                    .size(14)
                    .style(|_theme| text::Style {
                        color: Some(iced::Color::from_rgb(0.6, 0.6, 0.6)),
                    });

                let video_views = text(format!("Views: {}", video.views))
                    .size(14)
                    .style(|_theme| text::Style {
                        color: Some(iced::Color::from_rgb(0.6, 0.6, 0.6)),
                    });

                let video_url = button(
                    text(format!("🔗 {}", video.url()))
                        .size(12)
                )
                .on_press(Message::OpenUrl(video.url()))
                .padding(4)
                .style(|_theme, status| button::Style {
                    background: None,
                    text_color: match status {
                        button::Status::Hovered => iced::Color::from_rgb(0.5, 0.7, 1.0),
                        _ => iced::Color::from_rgb(0.4, 0.6, 0.9),
                    },
                    border: iced::Border {
                        color: iced::Color::TRANSPARENT,
                        width: 0.0,
                        radius: 0.0.into(),
                    },
                    shadow: iced::Shadow::default(),
                });

                let is_downloading = self.downloading.get(&video.video_id).copied().unwrap_or(false);
                let download_status = self.download_messages.get(&video.video_id);
                
                let download_button = button(
                    text(if is_downloading { "Downloading..." } else { "Download MP3" })
                        .size(14)
                )
                .on_press_maybe(if is_downloading {
                    None
                } else {
                    Some(Message::DownloadMp3(video.video_id.clone()))
                })
                .padding(8);
                
                let view_logs_button = if self.download_logs.contains_key(&video.video_id) {
                    Some(button(text("View Logs").size(12))
                        .on_press(Message::ShowLogs(video.video_id.clone()))
                        .padding(6))
                } else {
                    None
                };
                
                let mut info_column = column![
                    video_title,
                    video_channel,
                    video_duration,
                    video_views,
                    video_url,
                    download_button,
                ]
                .spacing(5)
                .width(Length::Fill);
                
                if let Some(logs_btn) = view_logs_button {
                    info_column = info_column.push(logs_btn);
                }
                
                // Show downloading indicator
                if is_downloading {
                    info_column = info_column.push(
                        text("⏳ Downloading...")
                            .size(14)
                            .style(|_theme| text::Style {
                                color: Some(iced::Color::from_rgb(0.4, 0.6, 0.9)),
                            })
                    );
                }
                
                if let Some(status) = download_status {
                    info_column = info_column.push(
                        text(status)
                            .size(12)
                            .style(|_theme| text::Style {
                                color: Some(if status.contains("Error") {
                                    iced::Color::from_rgb(0.8, 0.2, 0.2)
                                } else {
                                    iced::Color::from_rgb(0.2, 0.6, 0.2)
                                }),
                            })
                    );
                }
                
                let video_info = info_column;

                let content_row = if let Some(thumbnail_handle) = self.thumbnails.get(&video.video_id) {
                    // Show thumbnail with video info
                    row![
                        Image::new(thumbnail_handle.clone())
                            .width(120)
                            .height(90),
                        video_info,
                    ]
                    .spacing(15)
                } else {
                    // Show just video info while thumbnail loads
                    row![
                        container(text("Loading...").size(10))
                            .width(120)
                            .height(90)
                            .center_x(120)
                            .center_y(90)
                            .style(|_theme| container::Style {
                                background: Some(iced::Background::Color(iced::Color::from_rgb(0.2, 0.2, 0.22))),
                                ..Default::default()
                            }),
                        video_info,
                    ]
                    .spacing(15)
                };

                let video_container = container(content_row)
                    .padding(15)
                    .width(Length::Fill)
                    .style(|_theme| container::Style {
                        background: Some(iced::Background::Color(iced::Color::from_rgb(
                            0.15, 0.15, 0.18,
                        ))),
                        border: iced::Border {
                            color: iced::Color::from_rgb(0.25, 0.25, 0.3),
                            width: 1.0,
                            radius: 5.0.into(),
                        },
                        ..Default::default()
                    })
                    .width(Length::Fill);

                results_list = results_list.push(video_container);
            }

            let scrollable_results = scrollable(results_list)
                .width(Length::Fill)
                .id(self.results_scroll_id.clone());

            column![
                header,
                results_title,
                scrollable_results,
            ]
            .spacing(20)
            .padding(20)
        } else if self.is_searching {
            // Show loading indicator when searching
            column![
                header,
                text("Loading...")
                    .size(18)
                    .style(|_theme| text::Style {
                        color: Some(iced::Color::from_rgb(0.4, 0.6, 0.9)),
                    }),
            ]
            .spacing(10)
            .padding(20)
        } else if self.error_message.is_none() {
            column![
                header,
                text("Enter a search query or paste a YouTube URL/playlist above")
                    .size(16)
                    .style(|_theme| text::Style {
                        color: Some(iced::Color::from_rgb(0.6, 0.6, 0.6)),
                    }),
            ]
            .spacing(20)
            .padding(20)
        } else {
            column![header]
                .spacing(20)
                .padding(20)
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
    
    fn settings_view(&self) -> Element<'_, Message> {
        let title = text("Settings")
            .size(32);
        
        let back_button = button(text("← Back"))
            .on_press(Message::ToggleSettings)
            .padding(10);
        
        let header = row![back_button, title]
            .spacing(20)
            .width(Length::Fill);
        
        let dir_label = text("Download Directory:")
            .size(18);
        
        let dir_display = if let Some(dir) = &self.config.download_directory {
            text(dir.display().to_string())
                .size(14)
                .style(|_theme| text::Style {
                    color: Some(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                })
        } else {
            text("Not set")
                .size(14)
                .style(|_theme| text::Style {
                    color: Some(iced::Color::from_rgb(0.8, 0.2, 0.2)),
                })
        };
        
        let change_button = button(text("Choose Directory"))
            .on_press(Message::PickDirectory)
            .padding(10);
        
        // yt-dlp section
        let ytdlp_label = text("yt-dlp Binary:")
            .size(18);
        
        let ytdlp_path = get_ytdlp_path();
        let ytdlp_path_display = text(format!("Path: {}", ytdlp_path.display()))
            .size(14)
            .style(|_theme| text::Style {
                color: Some(iced::Color::from_rgb(0.5, 0.5, 0.5)),
            });
        
        let ytdlp_status_display = text(&self.ytdlp_status)
            .size(14)
            .style(|_theme| text::Style {
                color: Some(if self.ytdlp_status.contains("failed") || self.ytdlp_status.contains("not found") {
                    iced::Color::from_rgb(0.8, 0.2, 0.2)
                } else {
                    iced::Color::from_rgb(0.2, 0.6, 0.2)
                }),
            });
        
        let install_button = button(text(if self.ytdlp_installing { "Installing..." } else { "Install yt-dlp" }))
            .on_press_maybe(if self.ytdlp_installing || is_ytdlp_installed() {
                None
            } else {
                Some(Message::InstallYtDlp)
            })
            .padding(10);
        
        let player_logs_label = text("Player Logs:")
            .size(18);
        
        let player_logs_count = text(format!("{} log entries", self.player_logs.len()))
            .size(14)
            .style(|_theme| text::Style {
                color: Some(iced::Color::from_rgb(0.5, 0.5, 0.5)),
            });
        
        let view_logs_button = button(text("View Player Logs"))
            .on_press(Message::ShowPlayerLogs)
            .padding(10);
        
        let settings_content = column![
            header,
            column![
                dir_label,
                dir_display,
                change_button,
            ]
            .spacing(10)
            .padding(20),
            column![
                ytdlp_label,
                ytdlp_path_display,
                ytdlp_status_display,
                install_button,
            ]
            .spacing(10)
            .padding(20),
            column![
                player_logs_label,
                player_logs_count,
                view_logs_button,
            ]
            .spacing(10)
            .padding(20),
        ]
        .spacing(20)
        .width(Length::Fill);
        
        container(settings_content)
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
    
    fn logs_view(&self, video_id: &str) -> Element<'_, Message> {
        let title = text("Download Logs")
            .size(28);
        
        let back_button = button(text("← Back"))
            .on_press(Message::CloseLogs)
            .padding(10);
        
        let copy_button = button(text("📋 Copy to Clipboard"))
            .on_press(Message::CopyLogs(video_id.to_string()))
            .padding(10);
        
        let header = row![back_button, title, copy_button]
            .spacing(20)
            .width(Length::Fill);
        
        let logs = self.download_logs.get(video_id);
        
        let logs_content = if let Some(logs) = logs {
            let logs_text = logs.join("\n");
            scrollable(
                text(logs_text)
                    .size(12)
                    .style(|_theme| text::Style {
                        color: Some(iced::Color::from_rgb(0.9, 0.9, 0.9)),
                    })
            )
            .width(Length::Fill)
            .height(Length::Fill)
        } else {
            scrollable(text("No logs available").size(14))
                .width(Length::Fill)
                .height(Length::Fill)
        };
        
        let content = column![header, logs_content]
            .spacing(20)
            .width(Length::Fill)
            .height(Length::Fill);
        
        container(content)
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb(0.1, 0.1, 0.1))),
                ..Default::default()
            })
            .into()
    }
    
    fn rename_modal_view(&self, modal: &RenameModal) -> Element<'_, Message> {
        let title = text("Save As")
            .size(28);
        
        let instruction = text("Edit the filename and press Enter or click Download")
            .size(14)
            .style(|_theme| text::Style {
                color: Some(iced::Color::from_rgb(0.6, 0.6, 0.6)),
            });
        
        let filename_input = text_input("Filename (without extension)", &modal.filename)
            .on_input(Message::RenameFilenameChanged)
            .on_submit(Message::ConfirmDownload)
            .padding(10)
            .size(16)
            .width(Length::Fixed(500.0));
        
        let download_button = button(text("Download").size(16))
            .on_press(Message::ConfirmDownload)
            .padding(10);
        
        let cancel_button = button(text("Cancel").size(16))
            .on_press(Message::CancelRename)
            .padding(10);
        
        let buttons = row![cancel_button, download_button]
            .spacing(10);
        
        let modal_content = column![
            title,
            instruction,
            filename_input,
            buttons,
        ]
        .spacing(20)
        .padding(30)
        .max_width(600);
        
        container(modal_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.8))),
                ..Default::default()
            })
            .into()
    }
    
    fn player_logs_view(&self) -> Element<'_, Message> {
        let title = text("Player Logs")
            .size(28);
        
        let back_button = button(text("← Back"))
            .on_press(Message::ClosePlayerLogs)
            .padding(10);
        
        let copy_button = button(text("📋 Copy to Clipboard"))
            .on_press(Message::CopyPlayerLogs)
            .padding(10);
        
        let header = row![back_button, title, copy_button]
            .spacing(20)
            .width(Length::Fill);
        
        let logs_text = if self.player_logs.is_empty() {
            "No player logs yet. Try previewing a video.".to_string()
        } else {
            self.player_logs.join("\n")
        };
        
        let logs_content = scrollable(
            text(logs_text)
                .size(12)
                .style(|_theme| text::Style {
                    color: Some(iced::Color::from_rgb(0.9, 0.9, 0.9)),
                })
        )
        .width(Length::Fill)
        .height(Length::Fill);
        
        let content = column![header, logs_content]
            .spacing(20)
            .width(Length::Fill)
            .height(Length::Fill);
        
        container(content)
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb(0.1, 0.1, 0.1))),
                ..Default::default()
            })
            .into()
    }
}
