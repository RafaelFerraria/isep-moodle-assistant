//! Sincronizador do Moodle do ISEP: descobre as disciplinas e descarrega, de forma incremental, ficheiros,
//! pastas, enunciados de tarefas e páginas. Regista o que é novo/atualizado em `new_files.jsonl`.
use crate::dpapi;
use crate::history;
use crate::http::{decode_component, join_url, HttpClient, Response};
use crate::model::NewFile;
use crate::paths;
use crate::util::{now_iso, read_json, str_of, write_json_atomic, Json};
use chrono::Local;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

pub const BASE_URL: &str = "https://moodle.isep.ipp.pt";
const LOCK_MAX_AGE: Duration = Duration::from_secs(3600);

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}
re!(RE_SESSKEY_JS, r#"["']sesskey["']\s*:\s*["']([a-zA-Z0-9]+)["']"#);
re!(RE_SESSKEY_INPUT, r#"name=["']sesskey["']\s+value=["']([a-zA-Z0-9]+)["']"#);
re!(RE_COURSE_LINK, r"/course/view\.php\?id=(\d+)");
re!(RE_FILENAME_STAR, r"(?i)filename\*=(?:UTF-8|utf-8)''([^;]+)");
re!(RE_FILENAME, r#"filename=["']?([^";\r\n]+)"#);
re!(RE_INVALID_CHARS, r#"[<>:"/\\|?*]"#);
re!(RE_SECTION_CLASS, r"section\s+main|course-section");
re!(RE_SECTION_NAME_CLASS, r"sectionname|section-title");
re!(RE_FOLDER_ID, r"id=(\d+)");

// ---------------------------------------------------------------- utilitários

/// Limpa caracteres inválidos no Windows e corta espaços.
pub fn sanitize_filename(name: &str) -> String {
    let n = RE_INVALID_CHARS.replace_all(name, "_");
    let n = n.split_whitespace().collect::<Vec<_>>().join(" ");
    let n: String = n.chars().take(120).collect();
    if n.is_empty() { "sem_nome".into() } else { n }
}

pub fn clean_username(user: &str) -> String {
    let user = user.trim();
    match user.to_lowercase().find("@isep.ipp.pt") {
        Some(i) => user[..i].to_string(),
        None => user.to_string(),
    }
}

fn mime_extension(content_type: &str) -> Option<&'static str> {
    Some(match content_type.split(';').next()?.trim().to_lowercase().as_str() {
        "application/pdf" => ".pdf",
        "application/zip" | "application/x-zip-compressed" => ".zip",
        "application/msword" => ".doc",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => ".docx",
        "application/vnd.ms-excel" => ".xls",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => ".xlsx",
        "application/vnd.ms-powerpoint" => ".ppt",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => ".pptx",
        "text/plain" => ".txt",
        "text/csv" => ".csv",
        "application/json" => ".json",
        "image/png" => ".png",
        "image/jpeg" => ".jpg",
        "image/gif" => ".gif",
        "image/svg+xml" => ".svg",
        _ => return None,
    })
}

fn has_extension(name: &str) -> bool {
    Path::new(name).extension().is_some()
}

/// Nome real do ficheiro: Content-Disposition > URL final (pluginfile) > texto do link (+ extensão pelo Content-Type).
pub fn real_name(resp: &Response, fallback: &str) -> String {
    let disp = resp.header("content-disposition").unwrap_or("");
    let mut name: Option<String> = RE_FILENAME_STAR
        .captures(disp)
        .map(|m| decode_component(&m[1]))
        .or_else(|| RE_FILENAME.captures(disp).map(|m| m[1].trim().to_string()));
    if name.is_none() && resp.url.contains("pluginfile.php") {
        let path = resp.url.split(['?', '#']).next().unwrap_or("");
        let base = decode_component(path.rsplit('/').next().unwrap_or(""));
        if has_extension(&base) {
            name = Some(base);
        }
    }
    let mut name = name.unwrap_or_else(|| fallback.to_string());
    if !has_extension(&name) {
        let ctype = resp.header("content-type").unwrap_or("");
        if !ctype.to_lowercase().starts_with("text/html") {
            if let Some(ext) = mime_extension(ctype) {
                name.push_str(ext);
            }
        }
    }
    sanitize_filename(&name)
}

fn sha(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

/// Texto visível de um elemento (como `get_text(strip=True)` do BeautifulSoup).
fn text_of(el: &ElementRef) -> String {
    el.text().collect::<String>().trim().to_string()
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).unwrap()
}

// ---------------------------------------------------------------- motor

pub type LogSink = Box<dyn Fn(&str) + Send>;

#[derive(Default)]
pub struct SyncOutcome {
    pub ok: bool,
    pub busy: bool,
    pub new_files: Vec<NewFile>,
    pub errors: usize,
}

pub struct MoodleSync {
    pub base_url: String,
    username: String,
    password: String,
    pub download_dir: PathBuf,
    pub sync_dir: Option<PathBuf>,
    client: HttpClient,
    sesskey: Option<String>,
    pub errors: usize,
    pub new_files: Vec<NewFile>,
    pub busy: bool,
    ctx_course: String,
    ctx_section: String,
    sink: Option<LogSink>,
}

impl MoodleSync {
    pub fn new(username: &str, password: &str, download_dir: &Path, sync_dir: Option<&Path>, sink: Option<LogSink>) -> Result<Self, String> {
        Ok(MoodleSync {
            base_url: BASE_URL.to_string(),
            username: clean_username(username),
            password: password.to_string(),
            download_dir: download_dir.to_path_buf(),
            sync_dir: sync_dir.map(Path::to_path_buf),
            client: HttpClient::new(Duration::from_secs(30))?,
            sesskey: None,
            errors: 0,
            new_files: Vec::new(),
            busy: false,
            ctx_course: String::new(),
            ctx_section: String::new(),
            sink,
        })
    }

    pub fn log(&self, msg: &str) {
        let line = format!("[{}] {msg}", Local::now().format("%Y-%m-%d %H:%M:%S"));
        println!("{line}");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(paths::sync_log(self.sync_dir.as_deref())) {
            let _ = writeln!(f, "{line}");
        }
        if let Some(sink) = &self.sink {
            sink(msg);
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn find_sesskey(text: &str) -> Option<String> {
        RE_SESSKEY_JS.captures(text).or_else(|| RE_SESSKEY_INPUT.captures(text)).map(|m| m[1].to_string())
    }

    pub fn login(&mut self) -> bool {
        self.log("A autenticar no Moodle do ISEP...");
        let login_url = self.url("/login/index.php");
        // 1. carrega a página de login para obter os cookies iniciais
        if let Err(e) = self.client.get(&login_url, &[]) {
            self.log(&format!("Erro durante a autenticação: {e}"));
            return false;
        }
        // 2. envia as credenciais
        let resp = match self.client.post_form(&login_url, &[("username", &self.username), ("password", &self.password)]) {
            Ok(r) => r,
            Err(e) => {
                self.log(&format!("Erro durante a autenticação: {e}"));
                return false;
            }
        };
        let text = resp.text();
        // 3. verifica o sucesso
        if resp.url.contains("errorcode=3") || text.contains("divErro") || text.contains("Dados de acesso errados") {
            self.log("Erro: Credenciais de acesso erradas no Moodle ISEP!");
            return false;
        }
        self.sesskey = Self::find_sesskey(&text);
        if self.sesskey.is_none() {
            // a WinHTTP não expõe o frasco de cookies: confirma a sessão pedindo uma página que exige login
            if let Ok(page) = self.client.get(&self.url("/my/"), &[]) {
                self.sesskey = Self::find_sesskey(&page.text());
            }
        }
        if self.sesskey.is_none() {
            self.log("Erro: Sessão do Moodle não foi estabelecida.");
            return false;
        }
        self.log("Autenticação com sucesso!");
        true
    }

    /// Descobre todas as disciplinas em que o utilizador está inscrito, por ordem de descoberta.
    pub fn get_enrolled_courses(&mut self) -> Vec<(String, String)> {
        self.log("A pesquisar disciplinas inscritas...");
        let mut courses: Vec<(String, String)> = Vec::new();
        let add = |courses: &mut Vec<(String, String)>, id: String, name: String| {
            if id != "1" && !courses.iter().any(|(i, _)| *i == id) {
                courses.push((id, name));
            }
        };

        // Método A: API AJAX do Moodle
        if let Some(key) = self.sesskey.clone() {
            let method = "core_course_get_enrolled_courses_by_timeline_classification";
            let url = self.url(&format!("/lib/ajax/service.php?sesskey={key}&info={method}"));
            let payload = json!([{ "index": 0, "methodname": method,
                "args": { "offset": 0, "limit": 0, "classification": "all", "sort": "fullname" } }]);
            match self.client.post_json(&url, &[], &payload) {
                Ok(r) if r.status == 200 => {
                    if let Ok(data) = serde_json::from_slice::<Value>(&r.body) {
                        if let Some(list) = data[0]["data"]["courses"].as_array() {
                            for c in list {
                                let id = c["id"].as_i64().map(|i| i.to_string()).or_else(|| c["id"].as_str().map(str::to_owned)).unwrap_or_default();
                                let name = c["fullname"].as_str().filter(|s| !s.is_empty()).or_else(|| c["shortname"].as_str().filter(|s| !s.is_empty()))
                                    .map(str::to_owned).unwrap_or_else(|| format!("Disciplina_{id}"));
                                if !id.is_empty() {
                                    add(&mut courses, id, name);
                                }
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => self.log(&format!("Aviso ao consultar API AJAX de cursos: {e}")),
            }
        }

        // Método B: scraping da página /my/ e da página inicial
        for page_url in [self.url("/my/"), self.url("/")] {
            match self.client.get(&page_url, &[]) {
                Ok(r) if r.status == 200 => {
                    let html = r.text();
                    if self.sesskey.is_none() {
                        self.sesskey = Self::find_sesskey(&html);
                    }
                    let doc = Html::parse_document(&html);
                    for a in doc.select(&sel("a[href]")) {
                        let href = a.value().attr("href").unwrap_or("");
                        if let Some(m) = RE_COURSE_LINK.captures(href) {
                            let text = text_of(&a);
                            if text.chars().count() > 2 {
                                add(&mut courses, m[1].to_string(), text);
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => self.log(&format!("Aviso ao analisar {page_url}: {e}")),
            }
        }
        self.log(&format!("Foram encontradas {} disciplinas.", courses.len()));
        courses
    }

    fn record(&mut self, kind: &str, path: &Path, size: u64) {
        let rel = path.strip_prefix(&self.download_dir).unwrap_or(path).to_string_lossy().into_owned();
        let entry = NewFile {
            ts: now_iso(),
            kind: kind.to_string(),
            course: self.ctx_course.clone(),
            section: self.ctx_section.clone(),
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            path: rel,
            size,
        };
        if let Ok(line) = serde_json::to_string(&entry) {
            let log_path = paths::new_files_log(self.sync_dir.as_deref());
            if let Some(parent) = log_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path) {
                let _ = writeln!(f, "{line}");
            }
        }
        self.new_files.push(entry);
    }

    /// Descarrega de forma incremental; só regista como novo se for novo ou tiver mudado.
    pub fn download_file(&mut self, url: &str, dest: &Path) -> bool {
        let resp = match self.client.get(url, &[]) {
            Ok(r) => r,
            Err(e) => {
                self.errors += 1;
                self.log(&format!("  [!] Erro ao descarregar {url}: {e}"));
                return false;
            }
        };
        if resp.status != 200 {
            self.errors += 1;
            self.log(&format!("  [!] HTTP {} ao descarregar {url}", resp.status));
            return false;
        }
        let legacy = dest.to_path_buf(); // nome antigo (sem extensão) usado em versões anteriores
        let fallback = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let dest = dest.with_file_name(real_name(&resp, &fallback));

        let existed = dest.exists();
        if existed {
            if let Ok(old) = std::fs::read(&dest) {
                if sha(&old) == sha(&resp.body) {
                    return true; // igual ao que já existe
                }
            }
        }
        if let Some(parent) = dest.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                self.errors += 1;
                return false;
            }
        }
        let tmp = PathBuf::from(format!("{}.tmp", dest.display()));
        if let Err(e) = std::fs::write(&tmp, &resp.body).and_then(|_| {
            if existed {
                let _ = std::fs::remove_file(&dest);
            }
            std::fs::rename(&tmp, &dest)
        }) {
            self.errors += 1;
            self.log(&format!("  [!] Erro ao guardar {}: {e}", dest.display()));
            let _ = std::fs::remove_file(&tmp);
            return false;
        }
        // migração: apaga o ficheiro antigo sem extensão (ex.: "Enunciado LP3 V1.2Ficheiro")
        if legacy != dest && legacy.is_file() && legacy.extension().is_none() {
            let _ = std::fs::remove_file(&legacy);
        }
        self.record(if existed { "atualizado" } else { "novo" }, &dest, resp.body.len() as u64);
        self.log(&format!("  [+] {}: {}", if existed { "Atualizado" } else { "Novo ficheiro" }, dest.file_name().unwrap_or_default().to_string_lossy()));
        true
    }

    /// Guarda o texto de uma atividade "Página" (mod/page) e regista se for nova ou tiver mudado.
    pub fn save_page(&mut self, url: &str, title: &str, dest_dir: &Path) {
        let resp = match self.client.get(url, &[]) {
            Ok(r) if r.status == 200 => r,
            Ok(_) => return,
            Err(e) => {
                self.errors += 1;
                self.log(&format!("  [!] Erro ao guardar página {title}: {e}"));
                return;
            }
        };
        let doc = Html::parse_document(&resp.text());
        let text = doc
            .select(&sel("[role=main]"))
            .next()
            .map(|m| m.text().map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default();
        if text.is_empty() {
            return;
        }
        let path = dest_dir.join(format!("{}.txt", sanitize_filename(title)));
        let existed = path.exists();
        if existed && std::fs::read_to_string(&path).map(|old| old == text).unwrap_or(false) {
            return;
        }
        if std::fs::create_dir_all(dest_dir).and_then(|_| std::fs::write(&path, &text)).is_err() {
            self.errors += 1;
            return;
        }
        self.record(if existed { "atualizado" } else { "novo" }, &path, text.len() as u64);
        self.log(&format!("  [+] Página {}: {title}", if existed { "atualizada" } else { "nova" }));
    }

    fn extract_zip(&mut self, bytes: &[u8], target: &Path) -> bool {
        let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else { return false };
        for i in 0..archive.len() {
            let Ok(mut file) = archive.by_index(i) else { continue };
            let Some(rel) = file.enclosed_name() else { continue }; // ignora caminhos absolutos / com ".."
            let out = target.join(rel);
            if file.is_dir() {
                let _ = std::fs::create_dir_all(&out);
                continue;
            }
            if let Some(parent) = out.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut f) = std::fs::File::create(&out) {
                let _ = std::io::copy(&mut file, &mut f);
            }
        }
        true
    }

    /// Sincroniza todos os tópicos e materiais de uma disciplina.
    pub fn sync_course(&mut self, course_id: &str, course_name: &str) {
        let clean = sanitize_filename(course_name);
        let course_dir = self.download_dir.join(&clean);
        let _ = std::fs::create_dir_all(&course_dir);
        self.log(&format!("\n--- Sincronizando: {clean} (ID: {course_id}) ---"));

        let course_url = self.url(&format!("/course/view.php?id={course_id}"));
        let html = match self.client.get(&course_url, &[]) {
            Ok(r) if r.status == 200 => r.text(),
            _ => {
                self.log(&format!("Não foi possível aceder à disciplina {course_id}"));
                return;
            }
        };
        let doc = Html::parse_document(&html);

        // secções da disciplina (li/div cuja classe corresponde); senão a página inteira
        let mut sections: Vec<ElementRef> = doc
            .select(&sel("li, div"))
            .filter(|el| el.value().attr("class").map(|c| RE_SECTION_CLASS.is_match(c)).unwrap_or(false))
            .collect();
        let whole = doc.root_element();
        if sections.is_empty() {
            sections.push(whole);
        }

        let a_sel = sel("a[href]");
        for (idx, sec) in sections.iter().enumerate() {
            let s_idx = idx + 1;
            let sec_name = sec
                .select(&sel("h3, h4, span"))
                .find(|e| e.value().attr("class").map(|c| RE_SECTION_NAME_CLASS.is_match(c)).unwrap_or(false))
                .map(|e| text_of(&e))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| format!("Secção {s_idx:02}"));
            let sec_dir = course_dir.join(format!("{s_idx:02} - {}", sanitize_filename(&sec_name)));
            self.ctx_course = course_name.to_string();
            self.ctx_section = sec_name.clone();

            let links: Vec<(String, String)> = sec
                .select(&a_sel)
                .map(|a| (join_url(&course_url, a.value().attr("href").unwrap_or("")), text_of(&a)))
                .collect();
            let with = |needle: &str| links.iter().filter(move |(h, _)| h.contains(needle)).cloned().collect::<Vec<_>>();

            // 1. recursos individuais (PDFs, docs, ficheiros)
            for (href, label) in with("/mod/resource/view.php?id=").iter().filter(|(h, _)| RE_FOLDER_ID.is_match(h)) {
                let label = if label.is_empty() { format!("recurso_{s_idx}") } else { label.clone() };
                self.download_file(href, &sec_dir.join(sanitize_filename(&label)));
            }

            // 2. pastas (descarrega o ZIP da pasta; senão raspa os ficheiros da página)
            for (href, label) in with("/mod/folder/view.php?id=") {
                let Some(m) = RE_FOLDER_ID.captures(&href) else { continue };
                let folder_id = m[1].to_string();
                let name = if label.is_empty() { format!("Pasta_{folder_id}") } else { label };
                let target = sec_dir.join(sanitize_filename(&name));
                let zip_url = self.url(&format!("/mod/folder/download_folder.php?id={folder_id}"));
                if let Ok(z) = self.client.get(&zip_url, &[]) {
                    if z.status == 200 && z.header("content-type").unwrap_or("").to_lowercase().contains("zip") && self.extract_zip(&z.body, &target) {
                        self.log(&format!("  [+] Pasta sincronizada (ZIP): {name}"));
                        continue;
                    }
                }
                match self.client.get(&href, &[]) {
                    Ok(r) if r.status == 200 => {
                        let page = Html::parse_document(&r.text());
                        let files: Vec<(String, String)> = page
                            .select(&a_sel)
                            .filter(|a| a.value().attr("href").unwrap_or("").contains("pluginfile.php"))
                            .map(|a| (join_url(&href, a.value().attr("href").unwrap_or("")), text_of(&a)))
                            .collect();
                        for (furl, fname) in files {
                            let fname = if fname.is_empty() { "anexo".to_string() } else { fname };
                            self.download_file(&furl, &target.join(sanitize_filename(&fname)));
                        }
                    }
                    Ok(_) => {}
                    Err(e) => self.log(&format!("  [!] Erro ao processar pasta {name}: {e}")),
                }
            }

            // 3. enunciados de tarefas/trabalhos
            for (href, label) in with("/mod/assign/view.php?id=") {
                let name = if label.is_empty() { "Trabalho".to_string() } else { label };
                if let Ok(r) = self.client.get(&href, &[]) {
                    if r.status == 200 {
                        let page = Html::parse_document(&r.text());
                        let files: Vec<(String, String)> = page
                            .select(&a_sel)
                            .filter(|a| a.value().attr("href").unwrap_or("").contains("pluginfile.php"))
                            .map(|a| (join_url(&href, a.value().attr("href").unwrap_or("")), text_of(&a)))
                            .collect();
                        for (furl, fname) in files {
                            let fname = if fname.is_empty() { "enunciado".to_string() } else { fname };
                            self.download_file(&furl, &sec_dir.join("Trabalhos").join(sanitize_filename(&name)).join(sanitize_filename(&fname)));
                        }
                    }
                }
            }

            // 3b. páginas (texto no próprio Moodle: enunciados, avisos, guias)
            for (href, label) in with("/mod/page/view.php?id=") {
                let title = if label.is_empty() { "Pagina".to_string() } else { label };
                self.save_page(&href, &title, &sec_dir.join("Paginas"));
            }

            // 4. links externos: cria um atalho .url do Windows
            for (href, label) in with("/mod/url/view.php?id=") {
                let title = if label.is_empty() { "link".to_string() } else { label };
                if let Ok(r) = self.client.get(&href, &[]) {
                    let dir = sec_dir.join("Links");
                    let shortcut = dir.join(format!("{}.url", sanitize_filename(&title)));
                    if !shortcut.exists() && std::fs::create_dir_all(&dir).is_ok() {
                        let _ = std::fs::write(&shortcut, format!("[InternetShortcut]\nURL={}\n", r.url));
                    }
                }
            }
        }
    }

    /// Sincroniza todas as disciplinas. `false` se houve erros, se já havia outra sincronização ou se falhou o login.
    pub fn run_all(&mut self) -> bool {
        let lock = paths::sync_lock(self.sync_dir.as_deref());
        if let Some(parent) = lock.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let fresh = std::fs::metadata(&lock).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|age| age < LOCK_MAX_AGE).unwrap_or(false);
        if lock.exists() && fresh {
            self.log("Já existe uma sincronização em curso (sync.lock). A ignorar esta execução.");
            self.busy = true;
            return false;
        }
        let _ = std::fs::write(&lock, std::process::id().to_string());
        let result = self.run_all_locked();
        let _ = std::fs::remove_file(&lock);
        result
    }

    fn run_all_locked(&mut self) -> bool {
        if !self.login() {
            self.log("Falha ao iniciar sessão. A abortar sincronização.");
            return false;
        }
        let courses = self.get_enrolled_courses();
        if courses.is_empty() {
            self.log("Nenhuma disciplina encontrada.");
            return false;
        }
        self.log(&format!("Iniciando sincronização para {} disciplinas em '{}'...", courses.len(), self.download_dir.display()));
        for (id, name) in courses {
            // uma disciplina com erro não impede as restantes (os erros de rede contam-se em sync_course)
            self.sync_course(&id, &name);
        }
        self.log(&format!("Sincronização terminada: {} ficheiros novos/atualizados, {} erros.", self.new_files.len(), self.errors));
        self.errors == 0
    }
}

// ---------------------------------------------------------------- configuração e execução

pub fn load_sync_config(sync_dir: Option<&Path>) -> Json {
    read_json(&paths::sync_config(sync_dir))
}

pub fn save_sync_config(sync_dir: Option<&Path>, config: &Json) -> std::io::Result<()> {
    write_json_atomic(&paths::sync_config(sync_dir), config)
}

#[derive(Debug, Default, Clone)]
pub struct SyncResult {
    pub ok: bool,
    pub busy: bool,
    pub new_files: usize,
    pub errors: usize,
    pub error: Option<String>,
    pub files: Vec<NewFile>,
}

impl SyncResult {
    pub fn to_json(&self) -> Value {
        json!({
            "ok": self.ok, "busy": self.busy, "new_files": self.new_files, "errors": self.errors, "error": self.error,
            "files": self.files.iter().take(100).map(|f| json!({"course": f.course, "name": f.name, "kind": f.kind})).collect::<Vec<_>>(),
        })
    }
}

/// Sincroniza com a configuração guardada, atualiza `last_sync_date` e regista no histórico.
/// `base_url` serve para apontar a um Moodle de teste.
pub fn run_sync(trigger: &str, sync_dir: Option<&Path>, base_url: Option<&str>, progress: Option<LogSink>) -> SyncResult {
    let mut res = SyncResult::default();
    let started = now_iso();
    let mut config = load_sync_config(sync_dir);
    if str_of(&config, "username").is_empty() || str_of(&config, "encrypted_password").is_empty() {
        res.error = Some("Configuração em falta: define o utilizador e a palavra-passe do Moodle.".into());
        return res;
    }
    let password = match dpapi::decrypt(str_of(&config, "encrypted_password")) {
        Ok(p) => p,
        Err(e) => {
            res.error = Some(e);
            return res;
        }
    };
    let download_dir = match str_of(&config, "download_dir") {
        "" => paths::default_download_dir(),
        d => PathBuf::from(d),
    };
    match MoodleSync::new(str_of(&config, "username"), &password, &download_dir, sync_dir, progress) {
        Err(e) => res.error = Some(e),
        Ok(mut engine) => {
            if let Some(b) = base_url {
                engine.base_url = b.trim_end_matches('/').to_string();
            }
            res.ok = engine.run_all();
            res.busy = engine.busy;
            res.new_files = engine.new_files.len();
            res.errors = engine.errors;
            res.files = engine.new_files.iter().take(100).cloned().collect();
            if res.ok {
                config.insert("last_sync_date".into(), json!(Local::now().format("%Y-%m-%d").to_string()));
                let _ = save_sync_config(sync_dir, &config);
            } else if !res.busy {
                res.error = Some(if engine.errors > 0 {
                    format!("{} erro(s) durante a sincronização", engine.errors)
                } else {
                    "Falha ao iniciar sessão ou sem disciplinas.".into()
                });
            }
        }
    }
    if !res.busy {
        // uma execução ignorada por haver outra a decorrer não é uma execução
        let files: Vec<Value> = res.files.iter().take(40).map(|f| json!({"course": f.course, "name": f.name, "kind": f.kind})).collect();
        let rec = history::new_record("moodle_sync", trigger, res.ok, &started,
            Some(json!({"new_files": res.new_files, "errors": res.errors, "files": files})), None, res.error.as_deref());
        let _ = history::append(&rec);
    }
    res
}

/// Guarda o utilizador e a palavra-passe cifrada (DPAPI) depois de validar o login.
pub fn set_credentials(username: &str, password: &str, base_url: Option<&str>, sync_dir: Option<&Path>, download_dir: &Path) -> Result<String, String> {
    let user = clean_username(username);
    if user.is_empty() || password.is_empty() {
        return Err("Utilizador e palavra-passe são obrigatórios.".into());
    }
    let mut engine = MoodleSync::new(&user, password, download_dir, sync_dir, None)?;
    if let Some(b) = base_url {
        engine.base_url = b.trim_end_matches('/').to_string();
    }
    if !engine.login() {
        return Err("O Moodle recusou o login (confirma o número e a palavra-passe).".into());
    }
    let mut config = load_sync_config(sync_dir);
    config.insert("username".into(), json!(user));
    config.insert("encrypted_password".into(), json!(dpapi::encrypt(password)?));
    if str_of(&config, "download_dir").is_empty() {
        config.insert("download_dir".into(), json!(download_dir.to_string_lossy()));
    }
    config.entry("run_after_hour").or_insert(json!(18));
    config.entry("last_sync_date").or_insert(json!(""));
    save_sync_config(sync_dir, &config).map_err(|e| e.to_string())?;
    Ok(user)
}
