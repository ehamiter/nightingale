use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::net::{TcpListener, IpAddr};
use tiny_http::{Server, Response, Header};
use qrcode::QrCode;
use qrcode::render::unicode;

#[derive(Clone)]
pub struct ShareServer {
    file_path: PathBuf,
    filename: String,
    port: u16,
    running: Arc<Mutex<bool>>,
}

impl ShareServer {
    pub fn new<P: AsRef<Path>>(file_path: P) -> Result<Self, String> {
        let file_path = file_path.as_ref().to_path_buf();
        
        if !file_path.exists() {
            return Err("File does not exist".to_string());
        }
        
        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| "Invalid filename".to_string())?
            .to_string();
        
        // Find available port
        let port = Self::find_available_port()?;
        
        Ok(Self {
            file_path,
            filename,
            port,
            running: Arc::new(Mutex::new(false)),
        })
    }
    
    fn find_available_port() -> Result<u16, String> {
        let listener = TcpListener::bind("0.0.0.0:0")
            .map_err(|e| format!("Failed to bind to port: {}", e))?;
        
        let port = listener.local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?
            .port();
        
        Ok(port)
    }
    
    pub fn get_local_ip() -> Result<IpAddr, String> {
        // Get local IP address (not localhost)
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| format!("Failed to create socket: {}", e))?;
        
        socket.connect("8.8.8.8:80")
            .map_err(|e| format!("Failed to connect: {}", e))?;
        
        let local_addr = socket.local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?;
        
        Ok(local_addr.ip())
    }
    
    pub fn get_url(&self) -> Result<String, String> {
        let ip = Self::get_local_ip()?;
        Ok(format!("http://{}:{}", ip, self.port))
    }
    
    pub fn generate_qr_code(&self) -> Result<String, String> {
        let url = self.get_url()?;
        
        let code = QrCode::new(url.as_bytes())
            .map_err(|e| format!("Failed to generate QR code: {}", e))?;
        
        let qr_string = code.render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .build();
        
        Ok(qr_string)
    }
    
    pub fn start(&self) -> Result<(), String> {
        let addr = format!("0.0.0.0:{}", self.port);
        let server = Server::http(&addr)
            .map_err(|e| format!("Failed to start server: {}", e))?;
        
        let file_path = self.file_path.clone();
        let filename = self.filename.clone();
        let running = self.running.clone();
        
        // Set running to true
        {
            let mut r = running.lock().unwrap();
            *r = true;
        }
        
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                // Check if we should stop
                {
                    let r = running.lock().unwrap();
                    if !*r {
                        break;
                    }
                }
                
                let path = request.url();
                
                if path == "/" {
                    // Serve download page
                    let html = format!(
                        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Download {}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            max-width: 600px;
            margin: 50px auto;
            padding: 20px;
            text-align: center;
            background: #1a1a1a;
            color: #ffffff;
        }}
        h1 {{
            color: #4a9eff;
            margin-bottom: 30px;
        }}
        .download-btn {{
            display: inline-block;
            padding: 15px 40px;
            background: #4a9eff;
            color: white;
            text-decoration: none;
            border-radius: 8px;
            font-size: 18px;
            margin-top: 20px;
            transition: background 0.3s;
        }}
        .download-btn:hover {{
            background: #3a7edf;
        }}
        .filename {{
            background: #2a2a2a;
            padding: 15px;
            border-radius: 8px;
            margin: 20px 0;
            word-break: break-all;
            font-family: monospace;
        }}
        .info {{
            color: #888;
            margin-top: 30px;
            font-size: 14px;
        }}
    </style>
</head>
<body>
    <h1>🎵 Nightingale File Transfer</h1>
    <div class="filename">{}</div>
    <a href="/download" class="download-btn">Download MP3</a>
    <p class="info">The file will download to your device's Downloads folder.</p>
    <p class="info">Note: This will not add the file to the Music app. Use VLC or Files app for playback.</p>
</body>
</html>"#,
                        filename, filename
                    );
                    
                    let response = Response::from_string(html)
                        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
                    
                    let _ = request.respond(response);
                    
                } else if path == "/download" {
                    // Serve the file
                    match std::fs::read(&file_path) {
                        Ok(file_data) => {
                            let content_type = Header::from_bytes(&b"Content-Type"[..], &b"audio/mpeg"[..]).unwrap();
                            let content_disposition = Header::from_bytes(
                                &b"Content-Disposition"[..],
                                format!("attachment; filename=\"{}\"", filename).as_bytes()
                            ).unwrap();
                            
                            let response = Response::from_data(file_data)
                                .with_header(content_type)
                                .with_header(content_disposition);
                            
                            let _ = request.respond(response);
                        }
                        Err(_) => {
                            let response = Response::from_string("File not found")
                                .with_status_code(404);
                            let _ = request.respond(response);
                        }
                    }
                } else {
                    let response = Response::from_string("Not found")
                        .with_status_code(404);
                    let _ = request.respond(response);
                }
            }
        });
        
        Ok(())
    }
    
    pub fn stop(&self) {
        let mut r = self.running.lock().unwrap();
        *r = false;
    }
}
