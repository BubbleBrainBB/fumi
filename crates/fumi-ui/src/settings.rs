//! Preferences, kept in `NSUserDefaults` under the Swift app's key names, so a
//! build with the same bundle id reads what the user already chose.

use crate::theme::{AppTheme, AppearanceMode, DEFAULT_ACCENT};
use cocoa::base::{id, nil};
use cocoa::foundation::NSString;
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::CStr;

fn defaults() -> id {
    unsafe { msg_send![class!(NSUserDefaults), standardUserDefaults] }
}

fn ns(s: &str) -> id {
    unsafe { NSString::alloc(nil).init_str(s) }
}

fn from_ns(s: id) -> Option<String> {
    if s == nil {
        return None;
    }
    unsafe {
        let is_string: bool = msg_send![s, isKindOfClass: class!(NSString)];
        if !is_string {
            return None;
        }
        let utf8: *const std::os::raw::c_char = msg_send![s, UTF8String];
        (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }
}

fn object(key: &str) -> id {
    unsafe { msg_send![defaults(), objectForKey: ns(key)] }
}

pub fn has(key: &str) -> bool {
    object(key) != nil
}

pub fn remove(key: &str) {
    unsafe {
        let _: () = msg_send![defaults(), removeObjectForKey: ns(key)];
    }
}

pub fn get_string(key: &str) -> Option<String> {
    from_ns(object(key))
}

pub fn set_string(key: &str, value: &str) {
    unsafe {
        let _: () = msg_send![defaults(), setObject: ns(value) forKey: ns(key)];
    }
}

pub fn get_bool(key: &str, default: bool) -> bool {
    if !has(key) {
        return default;
    }
    unsafe { msg_send![defaults(), boolForKey: ns(key)] }
}

pub fn set_bool(key: &str, value: bool) {
    unsafe {
        let _: () = msg_send![defaults(), setBool: value forKey: ns(key)];
    }
}

pub fn get_f64(key: &str, default: f64) -> f64 {
    if !has(key) {
        return default;
    }
    unsafe { msg_send![defaults(), doubleForKey: ns(key)] }
}

pub fn set_f64(key: &str, value: f64) {
    unsafe {
        let _: () = msg_send![defaults(), setDouble: value forKey: ns(key)];
    }
}

pub fn get_i64(key: &str, default: i64) -> i64 {
    if !has(key) {
        return default;
    }
    unsafe { msg_send![defaults(), integerForKey: ns(key)] }
}

pub fn set_i64(key: &str, value: i64) {
    unsafe {
        let _: () = msg_send![defaults(), setInteger: value forKey: ns(key)];
    }
}

pub fn get_strings(key: &str) -> Option<Vec<String>> {
    let list = object(key);
    if list == nil {
        return None;
    }
    unsafe {
        let is_array: bool = msg_send![list, isKindOfClass: class!(NSArray)];
        if !is_array {
            return None;
        }
        let n: u64 = msg_send![list, count];
        Some((0..n).filter_map(|i| from_ns(msg_send![list, objectAtIndex: i])).collect())
    }
}

pub fn set_strings(key: &str, values: &[String]) {
    unsafe {
        let array: id = msg_send![class!(NSMutableArray), array];
        for v in values {
            let _: () = msg_send![array, addObject: ns(v)];
        }
        let _: () = msg_send![defaults(), setObject: array forKey: ns(key)];
    }
}

/// Key names, unchanged from `AppSettings.swift`.
pub mod keys {
    pub const FONT_SIZE: &str = "fumi.editorFontSize";
    pub const LINE_HEIGHT: &str = "fumi.editorLineHeight";
    pub const SORT_MODE: &str = "fumi.sortMode";
    pub const SIDEBAR_WIDTH: &str = "fumi.sidebarWidth";
    pub const LAUNCH_AT_LOGIN: &str = "fumi.launchAtLogin";
    pub const AUTO_UPDATES: &str = "fumi.autoUpdates";
    pub const ACCENT: &str = "fumi.accentColor";
    pub const WORKSPACE: &str = "fumi.workspacePath";
    pub const RECENT_WORKSPACES: &str = "fumi.recentWorkspaces";
    pub const OLD_APP_THEME: &str = "fumi.appTheme";
    pub const EDITOR_FONT: &str = "fumi.editorFont";
    pub const EDITOR_WIDTH: &str = "fumi.editorWidth";
    pub const SPELL_CHECK: &str = "fumi.spellCheck";
    pub const AUTO_PAIR: &str = "fumi.autoPair";
    pub const SMART_PUNCTUATION: &str = "fumi.smartPunctuation";
    pub const SLASH_COMMANDS: &str = "fumi.slashCommands";
    pub const MARKDOWN_SYNTAX: &str = "fumi.markdownSyntax";
    pub const TAB_SIZE: &str = "fumi.tabSize";
    pub const WORD_WRAP: &str = "fumi.wordWrap";
    pub const TYPEWRITER: &str = "fumi.typewriter";
    pub const EXPORT_FORMAT: &str = "fumi.exportFormat";
    pub const INCLUDE_FRONTMATTER: &str = "fumi.includeFrontmatter";
    pub const PRESERVE_TAGS: &str = "fumi.preserveTags";
    pub const IMAGE_HANDLING: &str = "fumi.imageHandling";
    pub const WECHAT_EXTENSION: &str = "fumi.extensions.wechat";
    pub const WECHAT_THEME: &str = "fumi.wechat.theme";
    pub const XHS_EXTENSION: &str = "fumi.extensions.xiaohongshu";
    pub const XHS_TEMPLATE: &str = "fumi.xhs.template";
    pub const XHS_RATIO: &str = "fumi.xhs.ratio";
    pub const XHS_PAGE_NUMBERS: &str = "fumi.xhs.pageNumbers";
    pub const XHS_NAME: &str = "fumi.xhs.name";
    pub const XHS_HANDLE: &str = "fumi.xhs.handle";
    pub const XHS_AVATAR: &str = "fumi.xhs.avatar";
    pub const XHS_END_CARD: &str = "fumi.xhs.endCard";
    pub const XHS_END_LINE: &str = "fumi.xhs.endLine";
    pub const COLOR_THEME: &str = "fumi.colorTheme";
    pub const APPEARANCE_MODE: &str = "fumi.appearanceMode";
}

pub fn color_theme() -> AppTheme {
    get_string(keys::COLOR_THEME)
        .and_then(|s| AppTheme::from_raw(&s))
        .unwrap_or(AppTheme::White)
}

pub fn appearance_mode() -> AppearanceMode {
    get_string(keys::APPEARANCE_MODE)
        .and_then(|s| AppearanceMode::from_raw(&s))
        .unwrap_or(AppearanceMode::Light)
}

pub fn accent_hex() -> String {
    get_string(keys::ACCENT).unwrap_or_else(|| DEFAULT_ACCENT.to_string())
}

pub fn editor_font() -> String {
    get_string(keys::EDITOR_FONT).unwrap_or_else(|| "System".to_string())
}

pub fn editor_width() -> String {
    get_string(keys::EDITOR_WIDTH).unwrap_or_else(|| "medium".to_string())
}

pub fn font_size() -> f64 {
    get_f64(keys::FONT_SIZE, 16.)
}

pub fn line_height() -> f64 {
    get_f64(keys::LINE_HEIGHT, 1.7)
}

pub fn auto_pair() -> bool {
    get_bool(keys::AUTO_PAIR, true)
}

pub fn smart_punctuation() -> bool {
    get_bool(keys::SMART_PUNCTUATION, false)
}

pub fn slash_commands() -> bool {
    get_bool(keys::SLASH_COMMANDS, true)
}

pub fn spell_check() -> bool {
    get_bool(keys::SPELL_CHECK, false)
}

pub fn markdown_syntax() -> String {
    get_string(keys::MARKDOWN_SYNTAX).unwrap_or_else(|| "focus".to_string())
}

pub fn workspace_path() -> String {
    get_string(keys::WORKSPACE).unwrap_or_default()
}

pub fn recent_workspaces() -> Vec<String> {
    get_strings(keys::RECENT_WORKSPACES).unwrap_or_else(|| {
        let last = workspace_path();
        if last.is_empty() { Vec::new() } else { vec![last] }
    })
}

pub fn note_recent_workspace(path: &str) {
    let mut list = vec![path.to_string()];
    list.extend(recent_workspaces().into_iter().filter(|p| p != path));
    list.truncate(5);
    set_strings(keys::RECENT_WORKSPACES, &list);
}

/// A value stored as data, as the Swift app keeps its encoded lists.
pub fn get_data(key: &str) -> Option<Vec<u8>> {
    unsafe {
        let data = object(key);
        if data == nil {
            return None;
        }
        let is_data: bool = msg_send![data, isKindOfClass: class!(NSData)];
        if !is_data {
            return None;
        }
        let len: u64 = msg_send![data, length];
        let bytes: *const u8 = msg_send![data, bytes];
        Some(std::slice::from_raw_parts(bytes, len as usize).to_vec())
    }
}

pub fn set_data(key: &str, value: &[u8]) {
    unsafe {
        let data: id = msg_send![class!(NSData), dataWithBytes: value.as_ptr() length: value.len() as u64];
        let _: () = msg_send![defaults(), setObject: data forKey: ns(key)];
    }
}

/// Reads `dictionary[sub]` as a string.
pub fn get_dict_string(key: &str, sub: &str) -> Option<String> {
    unsafe {
        let dict = object(key);
        if dict == nil {
            return None;
        }
        let is_dict: bool = msg_send![dict, isKindOfClass: class!(NSDictionary)];
        if !is_dict {
            return None;
        }
        from_ns(msg_send![dict, objectForKey: ns(sub)])
    }
}

/// Reads `dictionary[sub]` as data: how the Swift app keeps per-library values.
pub fn get_dict_data(key: &str, sub: &str) -> Option<Vec<u8>> {
    unsafe {
        let dict = object(key);
        if dict == nil {
            return None;
        }
        let is_dict: bool = msg_send![dict, isKindOfClass: class!(NSDictionary)];
        if !is_dict {
            return None;
        }
        let data: id = msg_send![dict, objectForKey: ns(sub)];
        if data == nil {
            return None;
        }
        let is_data: bool = msg_send![data, isKindOfClass: class!(NSData)];
        if !is_data {
            return None;
        }
        let len: u64 = msg_send![data, length];
        let bytes: *const u8 = msg_send![data, bytes];
        Some(std::slice::from_raw_parts(bytes, len as usize).to_vec())
    }
}

pub fn set_dict_data(key: &str, sub: &str, value: &[u8]) {
    unsafe {
        let old = object(key);
        let dict: id = if old != nil {
            msg_send![class!(NSMutableDictionary), dictionaryWithDictionary: old]
        } else {
            msg_send![class!(NSMutableDictionary), dictionary]
        };
        let data: id = msg_send![class!(NSData), dataWithBytes: value.as_ptr() length: value.len() as u64];
        let _: () = msg_send![dict, setObject: data forKey: ns(sub)];
        let _: () = msg_send![defaults(), setObject: dict forKey: ns(key)];
    }
}
