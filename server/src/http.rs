//! Cliente HTTP sobre a WinHTTP do Windows (TLS do sistema, proxy do sistema, cookies e redirecionamentos).
//!
//! Usa-se a WinHTTP em vez de `reqwest`/`rustls` porque estas dependências obrigam a um MinGW completo
//! para compilar com o toolchain GNU. Uma `HttpClient` mantém uma sessão (e o seu "frasco" de cookies),
//! como o `requests.Session` da versão Python — necessário para o login do Moodle.
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Response {
    pub status: u16,
    /// Cabeçalhos em minúsculas.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// URL final, depois dos redirecionamentos.
    pub url: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Partes de um URL http(s).
#[derive(Debug, PartialEq)]
pub struct UrlParts {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
}

pub fn parse_url(url: &str) -> Result<UrlParts, String> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(format!("URL não suportado: {url}"));
    };
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let path = path.split('#').next().unwrap_or("/");
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
            (h.to_string(), p.parse::<u16>().map_err(|_| format!("porta inválida em {url}"))?)
        }
        _ => (authority.to_string(), if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(format!("URL sem anfitrião: {url}"));
    }
    Ok(UrlParts { secure, host, port, path_and_query: path })
}

/// Junta um URL relativo ao URL base (para seguir links das páginas do Moodle).
pub fn join_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    let Ok(b) = parse_url(base) else { return href.to_string() };
    let origin = format!("{}://{}{}", if b.secure { "https" } else { "http" }, b.host,
        if (b.secure && b.port == 443) || (!b.secure && b.port == 80) { String::new() } else { format!(":{}", b.port) });
    if let Some(rest) = href.strip_prefix("//") {
        return format!("{}://{}", if b.secure { "https" } else { "http" }, rest);
    }
    if href.starts_with('/') {
        return format!("{origin}{href}");
    }
    let dir = match b.path_and_query.split('?').next().unwrap_or("/").rfind('/') {
        Some(i) => b.path_and_query[..=i].to_string(),
        None => "/".to_string(),
    };
    format!("{origin}{dir}{href}")
}

#[cfg(windows)]
mod imp {
    use super::{parse_url, Response};
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::time::Duration;
    use windows_sys::Win32::Networking::WinHttp::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { WinHttpCloseHandle(self.0) };
            }
        }
    }
    // A WinHTTP permite usar o handle de sessão a partir de várias threads.
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}

    pub struct Client {
        session: Handle,
    }

    fn last_error(what: &str) -> String {
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        let hint = match code {
            12002 => "tempo limite",
            12007 => "não foi possível resolver o nome (sem rede?)",
            12029 => "ligação recusada",
            12030 => "ligação terminada",
            12175 | 12157 => "falha de TLS",
            12038 | 12045 => "certificado inválido",
            _ => "",
        };
        if hint.is_empty() { format!("{what} (erro {code})") } else { format!("{what}: {hint} (erro {code})") }
    }

    impl Client {
        pub fn new(user_agent: &str, timeout: Duration) -> Result<Self, String> {
            unsafe {
                let ua = wide(user_agent);
                let mut session = WinHttpOpen(ua.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0);
                if session.is_null() {
                    // Windows antigos sem proxy automático
                    session = WinHttpOpen(ua.as_ptr(), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, null(), null(), 0);
                }
                if session.is_null() {
                    return Err(last_error("WinHttpOpen"));
                }
                let session = Handle(session);
                let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
                WinHttpSetTimeouts(session.0, 15_000, 15_000, ms, ms);
                let mut decompress: u32 = WINHTTP_DECOMPRESSION_FLAG_GZIP | WINHTTP_DECOMPRESSION_FLAG_DEFLATE;
                WinHttpSetOption(session.0, WINHTTP_OPTION_DECOMPRESSION, &mut decompress as *mut u32 as *const c_void, 4);
                Ok(Client { session })
            }
        }

        pub fn request(&self, method: &str, url: &str, headers: &[(&str, &str)], body: Option<&[u8]>) -> Result<Response, String> {
            let parts = parse_url(url)?;
            unsafe {
                let connect = Handle(WinHttpConnect(self.session.0, wide(&parts.host).as_ptr(), parts.port, 0));
                if connect.0.is_null() {
                    return Err(last_error("WinHttpConnect"));
                }
                let flags = if parts.secure { WINHTTP_FLAG_SECURE } else { 0 };
                let request = Handle(WinHttpOpenRequest(
                    connect.0, wide(method).as_ptr(), wide(&parts.path_and_query).as_ptr(),
                    null(), null(), null(), flags,
                ));
                if request.0.is_null() {
                    return Err(last_error("WinHttpOpenRequest"));
                }
                // seguir redirecionamentos (também de HTTPS para HTTP não: só os seguros)
                let mut policy: u32 = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
                WinHttpSetOption(request.0, WINHTTP_OPTION_REDIRECT_POLICY, &mut policy as *mut u32 as *const c_void, 4);

                let mut header_text = String::new();
                for (k, v) in headers {
                    header_text.push_str(&format!("{k}: {v}\r\n"));
                }
                let header_w: Vec<u16> = header_text.encode_utf16().collect();
                let (body_ptr, body_len) = match body {
                    Some(b) if !b.is_empty() => (b.as_ptr() as *const c_void, b.len() as u32),
                    _ => (null(), 0),
                };
                let sent = WinHttpSendRequest(
                    request.0,
                    if header_w.is_empty() { null() } else { header_w.as_ptr() },
                    header_w.len() as u32,
                    body_ptr, body_len, body_len, 0,
                );
                if sent == 0 {
                    return Err(last_error("WinHttpSendRequest"));
                }
                if WinHttpReceiveResponse(request.0, null_mut()) == 0 {
                    return Err(last_error("WinHttpReceiveResponse"));
                }

                // estado
                let mut status: u32 = 0;
                let mut size: u32 = 4;
                WinHttpQueryHeaders(
                    request.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, null(),
                    &mut status as *mut u32 as *mut c_void, &mut size, null_mut(),
                );

                // cabeçalhos em bruto
                let mut raw_size: u32 = 0;
                WinHttpQueryHeaders(request.0, WINHTTP_QUERY_RAW_HEADERS_CRLF, null(), null_mut(), &mut raw_size, null_mut());
                let mut raw = vec![0u16; (raw_size as usize / 2).max(1)];
                let mut headers_out = Vec::new();
                if WinHttpQueryHeaders(request.0, WINHTTP_QUERY_RAW_HEADERS_CRLF, null(), raw.as_mut_ptr() as *mut c_void, &mut raw_size, null_mut()) != 0 {
                    let text = String::from_utf16_lossy(&raw[..(raw_size as usize / 2).min(raw.len())]);
                    for line in text.split("\r\n").skip(1) {
                        if let Some((k, v)) = line.split_once(':') {
                            headers_out.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
                        }
                    }
                }

                // URL final
                let mut url_size: u32 = 0;
                WinHttpQueryOption(request.0, WINHTTP_OPTION_URL, null_mut(), &mut url_size);
                let mut url_buf = vec![0u16; (url_size as usize / 2).max(1)];
                let final_url = if WinHttpQueryOption(request.0, WINHTTP_OPTION_URL, url_buf.as_mut_ptr() as *mut c_void, &mut url_size) != 0 {
                    String::from_utf16_lossy(&url_buf[..(url_size as usize / 2).min(url_buf.len())]).trim_end_matches('\0').to_string()
                } else {
                    url.to_string()
                };

                // corpo
                let mut data = Vec::new();
                loop {
                    let mut available: u32 = 0;
                    if WinHttpQueryDataAvailable(request.0, &mut available) == 0 {
                        return Err(last_error("WinHttpQueryDataAvailable"));
                    }
                    if available == 0 {
                        break;
                    }
                    let start = data.len();
                    data.resize(start + available as usize, 0);
                    let mut read: u32 = 0;
                    if WinHttpReadData(request.0, data[start..].as_mut_ptr() as *mut c_void, available, &mut read) == 0 {
                        return Err(last_error("WinHttpReadData"));
                    }
                    data.truncate(start + read as usize);
                    if data.len() > 256 * 1024 * 1024 {
                        return Err("resposta demasiado grande".into());
                    }
                }
                Ok(Response { status: status as u16, headers: headers_out, body: data, url: final_url })
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Response;
    use std::time::Duration;

    pub struct Client;
    impl Client {
        pub fn new(_ua: &str, _t: Duration) -> Result<Self, String> {
            Err("O cliente HTTP só está implementado para Windows (WinHTTP).".into())
        }
        pub fn request(&self, _m: &str, _u: &str, _h: &[(&str, &str)], _b: Option<&[u8]>) -> Result<Response, String> {
            Err("O cliente HTTP só está implementado para Windows (WinHTTP).".into())
        }
    }
}

pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Sessão HTTP com cookies. Barata de criar; `Clone` não existe de propósito (um jar por sessão).
pub struct HttpClient {
    inner: imp::Client,
}

impl HttpClient {
    pub fn new(timeout: Duration) -> Result<Self, String> {
        Ok(HttpClient { inner: imp::Client::new(USER_AGENT, timeout)? })
    }

    pub fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Response, String> {
        self.inner.request("GET", url, headers, None)
    }

    pub fn post_form(&self, url: &str, fields: &[(&str, &str)]) -> Result<Response, String> {
        let body = fields
            .iter()
            .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
            .collect::<Vec<_>>()
            .join("&");
        self.inner.request("POST", url, &[("Content-Type", "application/x-www-form-urlencoded")], Some(body.as_bytes()))
    }

    pub fn post_json(&self, url: &str, headers: &[(&str, &str)], json: &serde_json::Value) -> Result<Response, String> {
        let mut h: Vec<(&str, &str)> = vec![("Content-Type", "application/json")];
        h.extend_from_slice(headers);
        self.inner.request("POST", url, &h, Some(json.to_string().as_bytes()))
    }

    pub fn request(&self, method: &str, url: &str, headers: &[(&str, &str)], body: Option<&[u8]>) -> Result<Response, String> {
        self.inner.request(method, url, headers, body)
    }
}

pub fn encode_component(s: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

pub fn decode_component(s: &str) -> String {
    percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned()
}
