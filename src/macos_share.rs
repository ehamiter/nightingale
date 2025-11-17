#![allow(unexpected_cfgs)]

#[cfg(target_os = "macos")]
use cocoa::base::{id, nil};
#[cfg(target_os = "macos")]
use objc::runtime::Class;
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};
#[cfg(target_os = "macos")]
use std::path::Path;

#[cfg(target_os = "macos")]
pub fn share_file_via_airdrop<P: AsRef<Path>>(file_path: P) -> Result<(), String> {
    unsafe {
        let app = cocoa::appkit::NSApp();
        
        // Get the key window
        let window: id = msg_send![app, keyWindow];
        if window == nil {
            return Err("No active window found".to_string());
        }
        
        // Convert file path to NSURL
        let path_str = file_path.as_ref().to_str()
            .ok_or_else(|| "Invalid file path".to_string())?;
        
        // Create NSString from Rust string
        let ns_string_class = Class::get("NSString").ok_or_else(|| "NSString class not found".to_string())?;
        let ns_path: id = msg_send![ns_string_class, stringWithUTF8String: path_str.as_ptr()];
        
        if ns_path == nil {
            return Err("Failed to create NSString".to_string());
        }
        
        let url_class = Class::get("NSURL").ok_or_else(|| "NSURL class not found".to_string())?;
        let file_url: id = msg_send![url_class, fileURLWithPath: ns_path];
        
        if file_url == nil {
            return Err("Failed to create file URL".to_string());
        }
        
        // Create NSArray with the file URL
        let array_class = Class::get("NSArray").ok_or_else(|| "NSArray class not found".to_string())?;
        let items: id = msg_send![array_class, arrayWithObject: file_url];
        
        // Get the sharing service picker class
        let picker_class = Class::get("NSSharingServicePicker")
            .ok_or_else(|| "NSSharingServicePicker class not found".to_string())?;
        
        // Create sharing service picker
        let picker: id = msg_send![picker_class, alloc];
        let picker: id = msg_send![picker, initWithItems: items];
        
        if picker == nil {
            return Err("Failed to create sharing service picker".to_string());
        }
        
        // Get the content view of the window
        let content_view: id = msg_send![window, contentView];
        
        // Create a rect for the sharing service picker location (centered)
        let frame: cocoa::foundation::NSRect = msg_send![content_view, bounds];
        let share_rect = cocoa::foundation::NSRect {
            origin: cocoa::foundation::NSPoint {
                x: frame.size.width / 2.0,
                y: frame.size.height / 2.0,
            },
            size: cocoa::foundation::NSSize {
                width: 1.0,
                height: 1.0,
            },
        };
        
        // Show the sharing service picker
        let _: () = msg_send![
            picker,
            showRelativeToRect: share_rect
            ofView: content_view
            preferredEdge: 3i64  // NSRectEdgeMaxY
        ];
        
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn share_file_via_airdrop<P: AsRef<std::path::Path>>(_file_path: P) -> Result<(), String> {
    Err("AirDrop is only available on macOS".to_string())
}
