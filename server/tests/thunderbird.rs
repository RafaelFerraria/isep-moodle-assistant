mod common;
use common::*;
use isep_assistant::api::{handle, App, Req};
use isep_assistant::thunderbird as tb;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const NAME: &str = "Thunderbird Setup 9.9.exe";

fn good_hash(bytes: &[u8]) -> String {
    tb::sha512_hex(bytes)
}

/// Mozilla de mentira: «latest» redireciona para o ficheiro da versão; há a lista SHA512SUMS e o instalador.
struct FakeMozilla {
    server: FakeServer,
}

fn fake_mozilla(installer: &'static [u8], sums_hash: String) -> FakeMozilla {
    let base: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let cell = base.clone();
    let server = fake_server(move |req| {
        let root = cell.lock().unwrap().clone();
        if req.url.starts_with("/latest") {
            Reply::bytes(302, b"", &[("Location", &format!("{root}/releases/9.9/win64/pt-PT/Thunderbird%20Setup%209.9.exe"))])
        } else if req.url.ends_with("/SHA512SUMS") {
            Reply::text(200, &format!("{sums_hash}  win64/pt-PT/{NAME}\n{}  win64/pt-PT/outro.exe\n", "0".repeat(128)))
        } else if req.url.contains("Thunderbird%20Setup%209.9.exe") || req.url.contains("Thunderbird Setup 9.9.exe") {
            Reply::bytes(200, installer, &[("Content-Type", "application/octet-stream")])
        } else {
            Reply::text(404, "não existe")
        }
    });
    *base.lock().unwrap() = server.base.clone();
    FakeMozilla { server }
}

fn env_for(home: &Home, m: &FakeMozilla) {
    std::env::set_var("ISEP_TB_DIR", home.path("tb"));
    std::env::set_var("ISEP_TB_LATEST_URL", format!("{}/latest", m.server.base));
    std::env::set_var("ISEP_TB_RELEASES_URL", format!("{}/releases", m.server.base));
    std::env::set_var("ISEP_TB_SKIP_SIGNATURE", "1");
}

fn clean_env() {
    for v in ["ISEP_TB_BUNDLE_DIR", "ISEP_TB_DIR", "ISEP_TB_LATEST_URL", "ISEP_TB_RELEASES_URL", "ISEP_TB_SKIP_SIGNATURE", "ISEP_THUNDERBIRD_PROFILES"] {
        std::env::remove_var(v);
    }
}

#[test]
fn expected_hash_reads_the_official_list_and_ignores_bad_lines() {
    let hash = "ab".repeat(64);
    let sums = format!("{hash}  win64/pt-PT/{NAME}\n{}  win64/pt-PT/other.exe\nlixo\n{}  win64/en-US/{NAME}\n", "cd".repeat(64), "ef".repeat(64));
    assert_eq!(tb::expected_hash(&sums, NAME), Some(hash));
    assert_eq!(tb::expected_hash(&sums, "Nao Existe.exe"), None);
    assert_eq!(tb::expected_hash(&format!("{}  win64/pt-PT/{NAME}", "zz".repeat(64)), NAME), None, "hash que não é hexadecimal");
    assert_eq!(tb::expected_hash(&format!("abc  win64/pt-PT/{NAME}"), NAME), None, "hash curto");
}

#[test]
fn installs_after_checking_the_hash_and_seeds_a_private_profile() {
    let home = Home::new();
    static BYTES: &[u8] = b"instalador falso";
    let m = fake_mozilla(BYTES, good_hash(BYTES));
    env_for(&home, &m);

    let lines = Mutex::new(Vec::<String>::new());
    let ran = Mutex::new(false);
    let res = tb::install_with(&|l| lines.lock().unwrap().push(l.to_string()), &|installer, target| {
        *ran.lock().unwrap() = true;
        assert!(installer.exists(), "o instalador tem de estar em disco quando corre");
        assert_eq!(std::fs::read(installer).unwrap(), BYTES);
        std::fs::create_dir_all(target).unwrap();
        std::fs::write(target.join("thunderbird.exe"), b"exe").unwrap();
        Ok(())
    });
    clean_env();
    let state = res.unwrap();
    assert!(*ran.lock().unwrap());
    assert_eq!(state["embedded"], true);
    assert_eq!(state["version"], "9.9");
    assert_eq!(state["profile_ready"], false, "ainda não foi adicionada nenhuma conta");
    let user_js = std::fs::read_to_string(home.path("tb/profile/user.js")).unwrap();
    assert!(user_js.contains("mail.shell.checkDefaultClient") && user_js.contains("toolkit.telemetry.enabled"));
    assert!(!home.path("tb/download/Thunderbird Setup 9.9.exe").exists(), "o instalador é apagado depois de usado");
    let policies: Value = serde_json::from_str(&std::fs::read_to_string(home.path("tb/app/distribution/policies.json")).unwrap()).unwrap();
    assert_eq!(policies["policies"]["DisableAppUpdate"], true);
    assert_eq!(policies["policies"]["DisableTelemetry"], true);
    assert!(lines.lock().unwrap().iter().any(|l| l.contains("9.9")));
}

#[test]
fn a_download_that_does_not_match_the_official_hash_is_never_run() {
    let home = Home::new();
    static BYTES: &[u8] = b"conteudo adulterado";
    let m = fake_mozilla(BYTES, good_hash(b"o original"));
    env_for(&home, &m);
    let ran = Mutex::new(false);
    let res = tb::install_with(&|_| {}, &|_, _| {
        *ran.lock().unwrap() = true;
        Ok(())
    });
    clean_env();
    let err = res.unwrap_err();
    assert!(err.contains("hash oficial"), "{err}");
    assert!(!*ran.lock().unwrap(), "um instalador que não bate certo nunca pode correr");
    assert!(!home.path("tb/download").exists() || std::fs::read_dir(home.path("tb/download")).unwrap().next().is_none());
}

#[test]
fn a_failed_installer_is_reported_and_leaves_nothing_behind() {
    let home = Home::new();
    static BYTES: &[u8] = b"instalador";
    let m = fake_mozilla(BYTES, good_hash(BYTES));
    env_for(&home, &m);
    let res = tb::install_with(&|_| {}, &|_, _| Err("o instalador terminou com erro".into()));
    clean_env();
    assert!(res.unwrap_err().contains("erro"));
    assert!(tb::embedded_exe().is_none());
    assert!(!home.path("tb/download/Thunderbird Setup 9.9.exe").exists());
}

#[test]
fn an_installer_that_does_not_produce_the_program_is_an_error() {
    let home = Home::new();
    static BYTES: &[u8] = b"instalador";
    let m = fake_mozilla(BYTES, good_hash(BYTES));
    env_for(&home, &m);
    let res = tb::install_with(&|_| {}, &|_, _| Ok(()));
    clean_env();
    assert!(res.unwrap_err().contains("não aparece"));
}

#[test]
fn the_apps_thunderbird_reads_its_own_profile_and_ignores_the_users() {
    let home = Home::new();
    std::env::set_var("ISEP_TB_DIR", home.path("tb"));
    // sem Thunderbird da app, o estado diz que não está instalado
    assert_eq!(tb::state()["embedded"], false);
    assert!(isep_assistant::mail::profiles_dir().to_string_lossy().contains("Thunderbird"));

    std::fs::create_dir_all(home.path("tb/app")).unwrap();
    std::fs::write(home.path("tb/app/thunderbird.exe"), b"exe").unwrap();
    assert_eq!(isep_assistant::mail::profiles_dir(), home.path("tb/profile"));
    assert_eq!(tb::state()["source"], "embedded");
    assert_eq!(tb::state()["profile_ready"], false);

    let inbox = home.path("tb/profile/x.default/ImapMail/outlook.office365.com/INBOX");
    std::fs::create_dir_all(inbox.parent().unwrap()).unwrap();
    std::fs::write(&inbox, "From - x\nSubject: a\n\ncorpo\n").unwrap();
    assert_eq!(tb::state()["profile_ready"], true, "a conta foi adicionada: há uma caixa sincronizada");
    clean_env();
}

#[test]
fn api_reports_the_state_and_refuses_to_open_what_is_not_installed() {
    let home = Home::new();
    std::env::set_var("ISEP_TB_DIR", home.path("tb"));
    let app = App::new("t");
    let call = |method: &str, path: &str| {
        handle(
            &app,
            &Req { method: method.into(), path: format!("/api/v1{path}"), query: vec![], host: Some("127.0.0.1:1".into()), authorization: Some("Bearer t".into()), body: vec![] },
        )
    };
    let st = call("GET", "/thunderbird");
    assert_eq!(st.status, 200);
    assert_eq!(st.body["embedded"], false);
    let open = call("POST", "/thunderbird/open");
    assert_eq!(open.status, 400);
    assert!(open.body["detail"].as_str().unwrap().contains("ainda não está instalado"));
    assert_eq!(call("GET", "/thunderbird/install").status, 405);
    let setup: Value = call("GET", "/setup").body;
    assert_eq!(setup["steps"].as_array().unwrap().iter().find(|s| s["id"] == "email").map(|s| s["id"].clone()), Some(json!("email")));
    clean_env();
}

#[test]
fn a_bundled_thunderbird_is_used_with_a_profile_outside_the_install() {
    let home = Home::new();
    // como numa app empacotada: resources/thunderbird/app/thunderbird.exe + version.txt
    std::fs::create_dir_all(home.path("res/thunderbird/app")).unwrap();
    std::fs::write(home.path("res/thunderbird/app/thunderbird.exe"), b"exe").unwrap();
    std::fs::write(home.path("res/thunderbird/version.txt"), "159.0
").unwrap();
    std::env::set_var("ISEP_TB_BUNDLE_DIR", home.path("res/thunderbird"));
    std::env::set_var("ISEP_TB_DIR", home.path("dados/Thunderbird"));

    let st = tb::state();
    assert_eq!(st["embedded"], true);
    assert_eq!(st["bundled"], true, "vem dentro da app: não há nada para instalar");
    assert_eq!(st["version"], "159.0");
    assert_eq!(st["source"], "embedded");
    assert_eq!(tb::embedded_exe().unwrap(), home.path("res/thunderbird/app/thunderbird.exe"));
    // a conta e os emails ficam na pasta de dados, não dentro da instalação da app
    assert_eq!(tb::profile_dir(), home.path("dados/Thunderbird/profile"));
    assert_eq!(isep_assistant::mail::profiles_dir(), home.path("dados/Thunderbird/profile"));
    assert!(!home.path("res/thunderbird/profile").exists());
    clean_env();
}

#[test]
fn without_a_bundle_the_state_says_it_is_not_bundled() {
    let home = Home::new();
    std::env::set_var("ISEP_TB_DIR", home.path("tb"));
    assert_eq!(tb::state()["bundled"], false);
    clean_env();
}

#[test]
fn the_installer_is_never_given_a_path_with_spaces() {
    let home = Home::new();
    // sem espaços: passa tal e qual
    let plain = home.path("semespacos");
    assert_eq!(tb::installer_target(&plain).unwrap(), plain.to_string_lossy());

    // com espaços (como «Nome Apelido»): ou vai na forma curta, sem espaços, ou recusa; nunca segue com espaços
    let spaced = home.path("com espaco/dentro");
    match tb::installer_target(&spaced) {
        Ok(path) => {
            assert!(!path.contains(' '), "{path}");
            assert!(std::path::Path::new(&path).is_dir(), "a forma curta tem de apontar para a mesma pasta");
            assert_eq!(std::fs::canonicalize(&path).unwrap(), std::fs::canonicalize(&spaced).unwrap());
        }
        Err(e) => assert!(e.contains("espaços") && e.contains("Não o executei"), "{e}"),
    }
}

#[test]
fn known_providers_get_a_preconfigured_oauth_account_and_unknown_ones_do_not() {
    assert_eq!(tb::provider_for("1234567@isep.ipp.pt").unwrap().imap_host, "outlook.office365.com");
    assert_eq!(tb::provider_for("ALUNO@Isep.IPP.pt").unwrap().imap_host, "outlook.office365.com");
    assert_eq!(tb::provider_for("x@gmail.com").unwrap().imap_host, "imap.gmail.com");
    assert!(tb::provider_for("x@universidade-desconhecida.pt").is_none());
    assert!(tb::provider_for("sem-arroba").is_none());
    assert!(tb::provider_for("x@evilisep.ipp.pt.fake.com").is_none(), "só o domínio exato ou subdomínios");

    let prefs = tb::account_prefs("1234567@isep.ipp.pt", &tb::provider_for("1234567@isep.ipp.pt").unwrap());
    assert!(prefs.contains(r#"user_pref("mail.server.server1.hostname", "outlook.office365.com");"#));
    assert!(prefs.contains(r#"user_pref("mail.server.server1.authMethod", 10);"#), "10 = OAuth2");
    assert!(prefs.contains(r#"user_pref("mail.server.server1.userName", "1234567@isep.ipp.pt");"#));
    assert!(prefs.contains(r#"user_pref("mail.server.server1.socketType", 3);"#), "3 = SSL/TLS");
    // um endereço malicioso não consegue fechar a aspa e injetar outras preferências
    let nasty = tb::account_prefs("a\"); user_pref(\"x\", 1); //@isep.ipp.pt", &tb::provider_for("a@isep.ipp.pt").unwrap());
    assert!(!nasty.contains("user_pref(\"x\", 1)"), "{nasty}");
}

#[test]
fn seeding_writes_the_account_only_for_known_providers_and_keeps_it_on_reseed() {
    let home = Home::new();
    std::env::set_var("ISEP_TB_DIR", home.path("tb"));
    assert_eq!(tb::seed_profile_for(Some("a@isep.ipp.pt")).unwrap(), true);
    let js = std::fs::read_to_string(home.path("tb/profile/user.js")).unwrap();
    assert!(js.contains("mail.server.server1.hostname") && js.contains("mail.shell.checkDefaultClient"));
    // voltar a preparar o perfil (ao abrir o Thunderbird) não apaga a conta
    tb::seed_profile().unwrap();
    assert!(std::fs::read_to_string(home.path("tb/profile/user.js")).unwrap().contains("mail.server.server1.hostname"));
    // fornecedor desconhecido: só as preferências base
    assert_eq!(tb::seed_profile_for(Some("a@escola-qualquer.pt")).unwrap(), false);
    assert!(!std::fs::read_to_string(home.path("tb/profile/user.js")).unwrap().contains("server1"));
    assert_eq!(tb::seed_profile_for(Some("não é um email")).unwrap(), false);
    clean_env();
}

#[test]
fn api_connect_account_validates_the_email_and_needs_the_app_thunderbird() {
    let home = Home::new();
    std::env::set_var("ISEP_TB_DIR", home.path("tb"));
    let app = App::new("t");
    let post = |body: Value| {
        handle(&app, &Req { method: "POST".into(), path: "/api/v1/thunderbird/account".into(), query: vec![], host: Some("127.0.0.1:1".into()),
            authorization: Some("Bearer t".into()), body: body.to_string().into_bytes() })
    };
    let bad = post(json!({"email": "sem-arroba"}));
    assert_eq!(bad.status, 422);
    assert!(bad.body["errors"]["email"].as_str().unwrap().contains("completo"));
    let none = post(json!({"email": "a@isep.ipp.pt"}));
    assert_eq!(none.status, 422);
    assert!(none.body["errors"]["email"].as_str().unwrap().contains("ainda não está instalado"));
    assert!(!home.path("tb/profile/user.js").exists(), "sem Thunderbird não se escreve nada");
    clean_env();
}
