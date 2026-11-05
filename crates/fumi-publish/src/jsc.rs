//! JavaScriptCore through its C API, as much as the WeChat renderer needs: one
//! context on one thread, and the host functions the bundle's node:fs and
//! node:crypto shims call (`Tools/wechat-renderer/shims`).

use std::ffi::c_void;
use std::ptr::null_mut;

type Ctx = *const c_void;
type Value = *const c_void;
type Obj = *mut c_void;
type JsStr = *mut c_void;
type Callback = unsafe extern "C" fn(Ctx, Obj, Obj, usize, *const Value, *mut Value) -> Value;

#[link(name = "JavaScriptCore", kind = "framework")]
unsafe extern "C" {
    fn JSGlobalContextCreate(class: *const c_void) -> *mut c_void;
    fn JSContextGetGlobalObject(ctx: Ctx) -> Obj;
    fn JSEvaluateScript(ctx: Ctx, script: JsStr, this: Obj, url: JsStr, line: i32, exception: *mut Value) -> Value;
    fn JSStringCreateWithCharacters(chars: *const u16, len: usize) -> JsStr;
    fn JSStringRelease(string: JsStr);
    fn JSStringGetLength(string: JsStr) -> usize;
    fn JSStringGetCharactersPtr(string: JsStr) -> *const u16;
    fn JSValueToStringCopy(ctx: Ctx, value: Value, exception: *mut Value) -> JsStr;
    fn JSValueMakeString(ctx: Ctx, string: JsStr) -> Value;
    fn JSValueMakeNull(ctx: Ctx) -> Value;
    fn JSValueMakeBoolean(ctx: Ctx, value: bool) -> Value;
    fn JSValueIsObject(ctx: Ctx, value: Value) -> bool;
    fn JSValueToObject(ctx: Ctx, value: Value, exception: *mut Value) -> Obj;
    fn JSValueMakeFromJSONString(ctx: Ctx, string: JsStr) -> Value;
    fn JSObjectMake(ctx: Ctx, class: *const c_void, data: *mut c_void) -> Obj;
    fn JSObjectIsFunction(ctx: Ctx, object: Obj) -> bool;
    fn JSObjectMakeFunctionWithCallback(ctx: Ctx, name: JsStr, callback: Callback) -> Obj;
    fn JSObjectGetProperty(ctx: Ctx, object: Obj, name: JsStr, exception: *mut Value) -> Value;
    fn JSObjectSetProperty(ctx: Ctx, object: Obj, name: JsStr, value: Value, attrs: u32, exception: *mut Value);
    fn JSObjectCallAsFunction(ctx: Ctx, function: Obj, this: Obj, count: usize, args: *const Value, exception: *mut Value) -> Value;
}

#[link(name = "System")]
unsafe extern "C" {
    fn CC_SHA256(data: *const c_void, len: u32, out: *mut u8) -> *mut u8;
}

fn js_string(text: &str) -> JsStr {
    let units: Vec<u16> = text.encode_utf16().collect();
    unsafe { JSStringCreateWithCharacters(units.as_ptr(), units.len()) }
}

fn rust_string(string: JsStr) -> String {
    unsafe {
        let len = JSStringGetLength(string);
        if len == 0 {
            return String::new();
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(JSStringGetCharactersPtr(string), len))
    }
}

fn to_rust(ctx: Ctx, value: Value) -> Option<String> {
    unsafe {
        let string = JSValueToStringCopy(ctx, value, null_mut());
        if string.is_null() {
            return None;
        }
        let out = rust_string(string);
        JSStringRelease(string);
        Some(out)
    }
}

fn get(ctx: Ctx, object: Obj, name: &str) -> Value {
    unsafe {
        let key = js_string(name);
        let value = JSObjectGetProperty(ctx, object, key, null_mut());
        JSStringRelease(key);
        value
    }
}

fn make_string(ctx: Ctx, text: &str) -> Value {
    unsafe {
        let string = js_string(text);
        let value = JSValueMakeString(ctx, string);
        JSStringRelease(string);
        value
    }
}

/// `exception.message`, or the exception itself.
fn exception_text(ctx: Ctx, exception: Value) -> String {
    unsafe {
        if JSValueIsObject(ctx, exception) {
            let object = JSValueToObject(ctx, exception, null_mut());
            let message = get(ctx, object, "message");
            if let Some(text) = to_rust(ctx, message).filter(|t| t != "undefined") {
                return text;
            }
        }
    }
    to_rust(ctx, exception).unwrap_or_default()
}

/// The thread's one host asset lookup, for the callback below.
static ASSETS: std::sync::OnceLock<fn(&str) -> Option<&'static str>> = std::sync::OnceLock::new();

unsafe fn first_arg(ctx: Ctx, count: usize, args: *const Value) -> String {
    if count == 0 {
        return String::new();
    }
    to_rust(ctx, unsafe { *args }).unwrap_or_default()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256(data: &[u8]) -> String {
    let mut out = [0u8; 32];
    unsafe { CC_SHA256(data.as_ptr().cast(), data.len() as u32, out.as_mut_ptr()) };
    hex(&out)
}

pub fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

fn nullable(ctx: Ctx, text: Option<String>) -> Value {
    match text {
        Some(text) => make_string(ctx, &text),
        None => unsafe { JSValueMakeNull(ctx) },
    }
}

unsafe extern "C" fn skill_asset(ctx: Ctx, _: Obj, _: Obj, count: usize, args: *const Value, _: *mut Value) -> Value {
    let relative = unsafe { first_arg(ctx, count, args) };
    let text = if relative.split('/').any(|p| p == "..") { None } else { ASSETS.get().and_then(|find| find(&relative)) };
    nullable(ctx, text.map(str::to_string))
}

unsafe extern "C" fn read_text(ctx: Ctx, _: Obj, _: Obj, count: usize, args: *const Value, _: *mut Value) -> Value {
    let path = unsafe { first_arg(ctx, count, args) };
    nullable(ctx, std::fs::read_to_string(path).ok())
}

unsafe extern "C" fn read_base64(ctx: Ctx, _: Obj, _: Obj, count: usize, args: *const Value, _: *mut Value) -> Value {
    let path = unsafe { first_arg(ctx, count, args) };
    nullable(ctx, std::fs::read(path).ok().map(|d| base64(&d)))
}

unsafe extern "C" fn file_exists(ctx: Ctx, _: Obj, _: Obj, count: usize, args: *const Value, _: *mut Value) -> Value {
    let path = unsafe { first_arg(ctx, count, args) };
    unsafe { JSValueMakeBoolean(ctx, std::path::Path::new(&path).exists()) }
}

unsafe extern "C" fn is_file(ctx: Ctx, _: Obj, _: Obj, count: usize, args: *const Value, _: *mut Value) -> Value {
    let path = unsafe { first_arg(ctx, count, args) };
    unsafe { JSValueMakeBoolean(ctx, std::path::Path::new(&path).is_file()) }
}

unsafe extern "C" fn sha256_text(ctx: Ctx, _: Obj, _: Obj, count: usize, args: *const Value, _: *mut Value) -> Value {
    let text = unsafe { first_arg(ctx, count, args) };
    make_string(ctx, &sha256(text.as_bytes()))
}

unsafe extern "C" fn sha256_file(ctx: Ctx, _: Obj, _: Obj, count: usize, args: *const Value, _: *mut Value) -> Value {
    let path = unsafe { first_arg(ctx, count, args) };
    make_string(ctx, &sha256(&std::fs::read(path).unwrap_or_default()))
}

/// One JavaScript context. Stays on the thread that made it.
pub struct Js {
    ctx: *mut c_void,
}

impl Js {
    pub fn new() -> Option<Js> {
        let ctx = unsafe { JSGlobalContextCreate(std::ptr::null()) };
        (!ctx.is_null()).then_some(Js { ctx })
    }

    fn ctx(&self) -> Ctx {
        self.ctx as Ctx
    }

    /// Sets `__fumiHost`, what the bundle's shims call.
    pub fn install_host(&self, assets: fn(&str) -> Option<&'static str>) {
        let _ = ASSETS.set(assets);
        let functions: [(&str, Callback); 7] = [
            ("skillAsset", skill_asset),
            ("readText", read_text),
            ("readBase64", read_base64),
            ("fileExists", file_exists),
            ("isFile", is_file),
            ("sha256Text", sha256_text),
            ("sha256File", sha256_file),
        ];
        unsafe {
            let ctx = self.ctx();
            let host = JSObjectMake(ctx, std::ptr::null(), null_mut());
            for (name, callback) in functions {
                let key = js_string(name);
                let function = JSObjectMakeFunctionWithCallback(ctx, key, callback);
                JSObjectSetProperty(ctx, host, key, function as Value, 0, null_mut());
                JSStringRelease(key);
            }
            let key = js_string("__fumiHost");
            JSObjectSetProperty(ctx, JSContextGetGlobalObject(ctx), key, host as Value, 0, null_mut());
            JSStringRelease(key);
        }
    }

    pub fn evaluate(&self, script: &str) -> Result<(), String> {
        unsafe {
            let ctx = self.ctx();
            let source = js_string(script);
            let mut exception: Value = std::ptr::null();
            JSEvaluateScript(ctx, source, null_mut(), null_mut(), 1, &mut exception);
            JSStringRelease(source);
            if exception.is_null() { Ok(()) } else { Err(exception_text(ctx, exception)) }
        }
    }

    fn method(&self, object: &str, name: &str) -> Option<(Obj, Obj)> {
        unsafe {
            let ctx = self.ctx();
            let holder = get(ctx, JSContextGetGlobalObject(ctx), object);
            if !JSValueIsObject(ctx, holder) {
                return None;
            }
            let holder = JSValueToObject(ctx, holder, null_mut());
            let function = get(ctx, holder, name);
            if !JSValueIsObject(ctx, function) {
                return None;
            }
            let function = JSValueToObject(ctx, function, null_mut());
            JSObjectIsFunction(ctx, function).then_some((holder, function))
        }
    }

    pub fn has_function(&self, object: &str, name: &str) -> bool {
        self.method(object, name).is_some()
    }

    /// `object.name(text, JSON.parse(options))`, as a string.
    pub fn call(&self, object: &str, name: &str, text: &str, options: &str) -> Result<String, String> {
        let (holder, function) = self.method(object, name).ok_or_else(|| "The WeChat renderer is missing its render function.".to_string())?;
        unsafe {
            let ctx = self.ctx();
            let json = js_string(options);
            let args = [make_string(ctx, text), JSValueMakeFromJSONString(ctx, json)];
            JSStringRelease(json);
            let mut exception: Value = std::ptr::null();
            let result = JSObjectCallAsFunction(ctx, function, holder, 2, args.as_ptr(), &mut exception);
            if !exception.is_null() {
                return Err(exception_text(ctx, exception));
            }
            to_rust(ctx, result).ok_or_else(|| "The WeChat renderer returned nothing.".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn sha256_matches_a_known_value() {
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
