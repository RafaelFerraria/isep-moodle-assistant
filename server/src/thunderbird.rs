//! Thunderbird da própria app: descarrega o instalador oficial da Mozilla, confere-o e instala-o numa pasta da app, com um
//! perfil só dele. O utilizador não instala nada e o Thunderbird que já tenha fica intocado.
//!
//! O Thunderbird continua a ser quem sincroniza a caixa (com a conta que a escola já aprovou) e a app só lê o que ele guarda.
//! Em vez de levar os ~300 MB dentro do instalador da app, descarrega-se na primeira vez: assim a versão é sempre a mais recente
//! e a marca Thunderbird é distribuída pela Mozilla, não por nós.
use crate::http::{decode_component, HttpClient};
use crate::paths;
use crate::util::run_with_timeout;
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha512};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const LATEST_URL: &str = "https://download.mozilla.org/?product=thunderbird-latest-ssl&os=win64&lang=pt-PT";
const RELEASES_URL: &str = "https://download-installer.cdn.mozilla.net/pub/thunderbird/releases";
const PLATFORM_DIR: &str = "win64/pt-PT";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(900);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

fn latest_url() -> String {
    std::env::var("ISEP_TB_LATEST_URL").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| LATEST_URL.into())
}
fn releases_url() -> String {
    std::env::var("ISEP_TB_RELEASES_URL").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| RELEASES_URL.into())
}

// ---------------------------------------------------------------- pastas

/// Onde vive o Thunderbird da app (programa + perfil). Fora do `Roaming`, porque são centenas de MB.
pub fn base_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("ISEP_TB_DIR") {
        return PathBuf::from(v);
    }
    match std::env::var_os("LOCALAPPDATA") {
        Some(local) => PathBuf::from(local).join("ISEP Assistant").join("Thunderbird"),
        None => paths::root().join("Thunderbird"),
    }
}
pub fn app_dir() -> PathBuf {
    base_dir().join("app")
}
pub fn profile_dir() -> PathBuf {
    base_dir().join("profile")
}
/// O Thunderbird que vem dentro da app (`resources/thunderbird/app`). `ISEP_TB_BUNDLE_DIR` só serve para os testes.
pub fn bundled_exe() -> Option<PathBuf> {
    let root = match std::env::var_os("ISEP_TB_BUNDLE_DIR") {
        Some(v) => PathBuf::from(v),
        None => paths::embedded_resources()?.join("thunderbird"),
    };
    let exe = root.join("app").join("thunderbird.exe");
    exe.is_file().then_some(exe)
}

/// O Thunderbird da app: o que vem dentro dela ou, em desenvolvimento, o que foi descarregado para a pasta da app.
/// O perfil (e por isso a conta e os emails) é sempre o da pasta de dados, fora da instalação.
pub fn embedded_exe() -> Option<PathBuf> {
    bundled_exe().or_else(|| {
        let exe = app_dir().join("thunderbird.exe");
        exe.is_file().then_some(exe)
    })
}
fn version_file() -> PathBuf {
    base_dir().join("version.txt")
}

/// O perfil da app já tem uma caixa de entrada sincronizada? (a conta foi adicionada)
pub fn profile_ready() -> bool {
    crate::mail::inboxes_in(&profile_dir()).first().is_some()
}

pub fn state() -> Value {
    let embedded = embedded_exe().is_some();
    json!({
        "embedded": embedded,
        "bundled": bundled_exe().is_some(),
        "version": std::fs::read_to_string(bundled_exe().and_then(|e| Some(e.parent()?.parent()?.join("version.txt"))).unwrap_or_else(version_file)).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
        "profile_ready": embedded && profile_ready(),
        "system_found": crate::system::find_system_thunderbird_path().is_some(),
        "source": if embedded { "embedded" } else if crate::system::find_system_thunderbird_path().is_some() { "system" } else { "none" },
    })
}

// ---------------------------------------------------------------- versão e verificação

pub struct Release {
    pub version: String,
    pub installer_url: String,
    pub sums_url: String,
    pub installer_name: String,
}

/// A versão mais recente: o endereço «latest» da Mozilla redireciona para o ficheiro da versão.
pub fn resolve_release(client: &HttpClient) -> Result<Release, String> {
    let resp = client.get(&latest_url(), &[])?;
    if resp.status != 200 {
        return Err(format!("A Mozilla não respondeu (HTTP {}).", resp.status));
    }
    let name = resp.url.rsplit('/').next().map(decode_component).unwrap_or_default();
    let re = Regex::new(r"^Thunderbird Setup ([0-9][0-9.]*)\.exe$").unwrap();
    let version = re.captures(&name).map(|c| c[1].to_string()).ok_or("Não consegui perceber a versão mais recente do Thunderbird.")?;
    // o redirecionamento já aponta para o ficheiro; os nomes guardam-se para validar contra a lista de verificação
    Ok(Release {
        sums_url: format!("{}/{version}/SHA512SUMS", releases_url()),
        installer_url: resp.url.clone(),
        installer_name: name,
        version,
    })
}

/// Hash esperado do instalador, lido da lista oficial (`SHA512SUMS`).
pub fn expected_hash(sums: &str, installer_name: &str) -> Option<String> {
    let wanted = format!("{PLATFORM_DIR}/{installer_name}");
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once("  ")?;
        (file.trim() == wanted && hash.len() == 128 && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| hash.to_lowercase())
    })
}

pub fn sha512_hex(data: &[u8]) -> String {
    hex::encode(Sha512::digest(data))
}

/// O instalador tem de estar assinado pela Mozilla (a assinatura digital do Windows é válida).
fn verify_signature(path: &Path) -> Result<(), String> {
    if std::env::var_os("ISEP_TB_SKIP_SIGNATURE").is_some() {
        return Ok(());
    }
    let ps = format!(
        "$s = Get-AuthenticodeSignature -LiteralPath '{}'; \"$($s.Status)|$($s.SignerCertificate.Subject)\"",
        path.display().to_string().replace('\'', "''")
    );
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &ps]);
    let out = run_with_timeout(cmd, None, Duration::from_secs(60)).map_err(|e| format!("não foi possível verificar a assinatura: {e}"))?;
    let line = out.stdout.trim();
    if line.starts_with("Valid|") && line.contains("Mozilla Corporation") {
        Ok(())
    } else {
        Err(format!("A assinatura digital do instalador não é a da Mozilla ({}). Não o instalei.", line.chars().take(120).collect::<String>()))
    }
}

// ---------------------------------------------------------------- instalação

/// Preferências do perfil da app: sem janelas de boas-vindas nem telemetria, e sem pedir para ser o programa de email por omissão.
const USER_JS: &str = r#"user_pref("mail.shell.checkDefaultClient", false);
user_pref("mail.provider.suppress_dialog_on_startup", true);
user_pref("mail.spotlight.firstRunDone", true);
user_pref("mail.winsearch.firstRunDone", true);
user_pref("mail.biff.show_alert", false);
user_pref("mail.biff.play_sound", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("datareporting.healthreport.uploadEnabled", false);
user_pref("toolkit.telemetry.enabled", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("app.donation.eoy.version.viewed", 999);
"#;

/// Políticas do próprio Thunderbird (suportadas pela Mozilla): sem atualizações automáticas (a versão vem com a app),
/// sem telemetria e sem a página de boas-vindas nem o pedido para ser o programa de email por omissão.
const POLICIES_JSON: &str = r#"{"policies":{"DisableAppUpdate":true,"DisableTelemetry":true,"DontCheckDefaultClient":true,"OverrideFirstRunPage":"","OverridePostUpdatePage":"","NoDefaultBookmarks":true}}"#;

fn write_policies(app: &Path) -> std::io::Result<()> {
    let dir = app.join("distribution");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("policies.json"), POLICIES_JSON)
}

/// Servidor de correio de uma conta que o Thunderbird sabe usar com início de sessão da Microsoft/Google (OAuth2).
pub struct Provider {
    pub imap_host: &'static str,
    pub smtp_host: &'static str,
}

/// O fornecedor do email. Só os conhecidos: para os outros o Thunderbird descobre a configuração sozinho no assistente dele.
pub fn provider_for(email: &str) -> Option<Provider> {
    let domain = email.rsplit_once('@')?.1.trim().to_lowercase();
    let microsoft = ["isep.ipp.pt", "outlook.com", "hotmail.com", "live.com", "office365.com"];
    if microsoft.iter().any(|d| domain == *d || domain.ends_with(&format!(".{d}"))) {
        return Some(Provider { imap_host: "outlook.office365.com", smtp_host: "smtp.office365.com" });
    }
    if domain == "gmail.com" || domain == "googlemail.com" {
        return Some(Provider { imap_host: "imap.gmail.com", smtp_host: "smtp.gmail.com" });
    }
    None
}

const ACCOUNT_BEGIN: &str = "// >>> conta da app (gerada)";
const ACCOUNT_END: &str = "// <<< conta da app";

/// As preferências que criam a conta no perfil da app (IMAP com OAuth2), para o utilizador não passar pelo assistente do Thunderbird.
pub fn account_prefs(email: &str, provider: &Provider) -> String {
    let user: String = email.trim().chars().filter(|c| *c != '\\' && *c != '"').collect();
    let name = user.split('@').next().unwrap_or("").to_string();
    let p = |k: &str, v: &str| format!("user_pref(\"{k}\", {v});\n");
    let s = |k: &str, v: &str| p(k, &format!("\"{v}\""));
    let mut out = format!("{ACCOUNT_BEGIN}\n");
    out += &s("mail.accountmanager.accounts", "account1");
    out += &s("mail.accountmanager.defaultaccount", "account1");
    out += &s("mail.account.account1.identities", "id1");
    out += &s("mail.account.account1.server", "server1");
    out += &s("mail.identity.id1.fullName", &name);
    out += &s("mail.identity.id1.useremail", &user);
    out += &s("mail.identity.id1.smtpServer", "smtp1");
    out += &s("mail.server.server1.type", "imap");
    out += &s("mail.server.server1.hostname", provider.imap_host);
    out += &p("mail.server.server1.port", "993");
    out += &p("mail.server.server1.socketType", "3");
    out += &s("mail.server.server1.userName", &user);
    out += &s("mail.server.server1.name", &user);
    out += &p("mail.server.server1.authMethod", "10");
    out += &p("mail.server.server1.check_new_mail", "true");
    out += &p("mail.server.server1.login_at_startup", "true");
    out += &p("mail.server.server1.autosync_offline_stores", "true");
    out += &s("mail.smtpservers", "smtp1");
    out += &s("mail.smtpserver.smtp1.hostname", provider.smtp_host);
    out += &p("mail.smtpserver.smtp1.port", "587");
    out += &p("mail.smtpserver.smtp1.try_ssl", "2");
    out += &p("mail.smtpserver.smtp1.authMethod", "10");
    out += &s("mail.smtpserver.smtp1.username", &user);
    out += ACCOUNT_END;
    out.push('\n');
    out
}

fn valid_email(email: &str) -> bool {
    let e = email.trim();
    let re = Regex::new(r"^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$").unwrap();
    re.is_match(e) && e.len() <= 120
}

/// Escreve o perfil base e, se o fornecedor for conhecido, a conta. Devolve se a conta foi pré-configurada.
pub fn seed_profile_for(email: Option<&str>) -> std::io::Result<bool> {
    std::fs::create_dir_all(profile_dir())?;
    let mut body = USER_JS.to_string();
    let mut seeded = false;
    if let Some(email) = email.filter(|e| valid_email(e)) {
        if let Some(provider) = provider_for(email) {
            body.push_str(&account_prefs(email.trim(), &provider));
            seeded = true;
        }
    }
    std::fs::write(profile_dir().join("user.js"), body)?;
    Ok(seeded)
}

pub fn seed_profile() -> std::io::Result<()> {
    // mantém a conta já pré-configurada, se houver
    let current = std::fs::read_to_string(profile_dir().join("user.js")).unwrap_or_default();
    let account = current.find(ACCOUNT_BEGIN).map(|i| current[i..].to_string());
    std::fs::create_dir_all(profile_dir())?;
    std::fs::write(profile_dir().join("user.js"), format!("{USER_JS}{}", account.unwrap_or_default()))
}

/// Pré-configura a conta da escola no Thunderbird da app e abre-o: o utilizador só tem de entrar na conta Microsoft.
pub fn connect_account(email: &str) -> Result<Value, String> {
    if !valid_email(email) {
        return Err("Escreve o teu email da escola completo (ex.: numero@isep.ipp.pt).".into());
    }
    if embedded_exe().is_none() {
        return Err("O Thunderbird da app ainda não está instalado.".into());
    }
    let seeded = seed_profile_for(Some(email)).map_err(|e| format!("não foi possível preparar o perfil: {e}"))?;
    open_for_account()?;
    Ok(json!({"ok": true, "preconfigured": seeded}))
}

/// Descarrega, confere e (com `run_installer`) instala. `run_installer` recebe o instalador e a pasta de destino.
pub fn install_with(log: &dyn Fn(&str), run_installer: &dyn Fn(&Path, &Path) -> Result<(), String>) -> Result<Value, String> {
    let client = HttpClient::new(DOWNLOAD_TIMEOUT)?;
    log("A procurar a versão mais recente do Thunderbird…");
    let release = resolve_release(&client)?;
    log(&format!("Versão {} encontrada.", release.version));

    let sums = client.get(&release.sums_url, &[])?;
    if sums.status != 200 {
        return Err(format!("Não consegui obter a lista de verificação da Mozilla (HTTP {}).", sums.status));
    }
    let expected = expected_hash(&sums.text(), &release.installer_name).ok_or("O instalador não consta da lista de verificação da Mozilla.")?;

    log("A descarregar o instalador (cerca de 85 MB)…");
    let installer = client.get(&release.installer_url, &[])?;
    if installer.status != 200 {
        return Err(format!("O descarregamento falhou (HTTP {}).", installer.status));
    }
    log("A verificar a integridade…");
    if sha512_hex(&installer.body) != expected {
        return Err("O ficheiro descarregado não corresponde ao hash oficial da Mozilla. Não o instalei.".into());
    }

    let dir = base_dir().join("download");
    std::fs::create_dir_all(&dir).map_err(|e| format!("não foi possível criar a pasta de descarga: {e}"))?;
    let path = dir.join(&release.installer_name);
    std::fs::write(&path, &installer.body).map_err(|e| format!("não foi possível guardar o instalador: {e}"))?;
    verify_signature(&path)?;
    log("Assinatura da Mozilla confirmada. A instalar…");

    let result = run_installer(&path, &app_dir());
    let _ = std::fs::remove_file(&path); // o instalador já não é preciso
    result?;
    if embedded_exe().is_none() {
        return Err("A instalação terminou mas o Thunderbird não aparece na pasta da app.".into());
    }
    write_policies(&app_dir()).map_err(|e| format!("não foi possível aplicar as políticas: {e}"))?;
    seed_profile().map_err(|e| format!("não foi possível preparar o perfil: {e}"))?;
    let _ = std::fs::write(version_file(), &release.version);
    log("Thunderbird instalado.");
    Ok(state())
}

/// O instalador da Mozilla só entende `/InstallDirectoryPath=` quando o caminho não tem espaços; com espaços ignora-o
/// **e instala no sítio por omissão, por cima de um Thunderbird que o utilizador já tenha**. Por isso usa-se a forma curta
/// do caminho (sem espaços) e, se não houver, recusa-se correr o instalador.
pub fn installer_target(target: &Path) -> Result<String, String> {
    std::fs::create_dir_all(target).map_err(|e| format!("não foi possível criar a pasta de destino: {e}"))?;
    let plain = target.to_string_lossy().into_owned();
    if !plain.contains(' ') {
        return Ok(plain);
    }
    let short = short_path(target).unwrap_or_default();
    if short.is_empty() || short.contains(' ') {
        return Err(format!(
            "O caminho «{plain}» tem espaços e o instalador da Mozilla não o aceitaria (instalaria noutro sítio). Não o executei."
        ));
    }
    Ok(short)
}

#[cfg(windows)]
fn short_path(p: &Path) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("cmd");
    cmd.raw_arg(format!("/c for %I in (\"{}\") do @echo %~sI", p.display()));
    let out = run_with_timeout(cmd, None, Duration::from_secs(20)).ok()?;
    Some(out.stdout.trim().to_string())
}
#[cfg(not(windows))]
fn short_path(_: &Path) -> Option<String> {
    None
}

/// Instalação real: o instalador oficial em silêncio, só para este utilizador e sem atalhos nem serviço de atualização.
pub fn install(log: &dyn Fn(&str)) -> Result<Value, String> {
    install_with(log, &|installer, target| {
        let dir = installer_target(target)?;
        let mut cmd = Command::new(installer);
        cmd.arg("/S")
            .arg(format!("/InstallDirectoryPath={dir}"))
            .args(["/MaintenanceService=false", "/TaskbarShortcut=false", "/DesktopShortcut=false", "/StartMenuShortcut=false", "/PreventRebootRequired=true"]);
        let out = run_with_timeout(cmd, None, INSTALL_TIMEOUT).map_err(|e| format!("não foi possível correr o instalador: {e}"))?;
        if out.timed_out {
            return Err("A instalação demorou demasiado.".into());
        }
        match out.status {
            Some(0) => Ok(()),
            other => Err(format!("O instalador terminou com erro ({other:?}).")),
        }
    })
}

/// Abre o Thunderbird da app, com o perfil da app, para o utilizador adicionar a conta da escola (uma vez).
pub fn open_for_account() -> Result<(), String> {
    let exe = embedded_exe().ok_or("O Thunderbird da app ainda não está instalado.")?;
    seed_profile().map_err(|e| format!("não foi possível preparar o perfil: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.arg("-profile").arg(profile_dir()).arg("-no-remote").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().map(|_| ()).map_err(|e| format!("não foi possível abrir o Thunderbird: {e}"))
}
