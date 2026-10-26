mod common;
use common::*;
use isep_assistant::history;
use isep_assistant::http::Response;
use isep_assistant::moodle_sync::*;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

const LAB: &str = "DEI - Laboratório / Projeto III - 1º Semestre 2026/2027";

fn zip_bytes() -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        z.start_file("a.txt", opts).unwrap();
        z.write_all(b"ficheiro a").unwrap();
        z.start_file("sub/b.txt", opts).unwrap();
        z.write_all(b"ficheiro b").unwrap();
        z.start_file("../evil.txt", opts).unwrap();
        z.write_all(b"zip slip").unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

struct Moodle {
    srv: FakeServer,
    pdf: Arc<Mutex<Vec<u8>>>,
}

fn fake_moodle() -> Moodle {
    let pdf = Arc::new(Mutex::new(b"%PDF-1.4 versao 1".to_vec()));
    let pdf2 = pdf.clone();
    let zip = zip_bytes();
    let srv = fake_server(move |s| {
        let path = s.url.split('?').next().unwrap().to_string();
        match (s.method.as_str(), path.as_str()) {
            ("GET", "/login/index.php") => Reply::text(200, "<html>login</html>").with("Set-Cookie", "MoodleSession=s1; Path=/"),
            ("POST", "/login/index.php") => {
                if s.body.contains("password=boa") {
                    Reply::text(200, r#"<html><script>M.cfg = {"sesskey":"abc123XYZ"};</script></html>"#)
                } else {
                    Reply::text(200, r#"<div class="divErro">Dados de acesso errados</div>"#)
                }
            }
            ("POST", "/lib/ajax/service.php") => Reply::json(
                200,
                json!([{"error": false, "data": {"courses": [{"id": 4235, "fullname": LAB}, {"id": 1, "fullname": "Site"}]}}]),
            ),
            ("GET", "/my/") => Reply::text(
                200,
                r#"<a href="/course/view.php?id=4235">DEI - Laboratório / Projeto III - 1º Semestre 2026/2027</a>
                   <a href="/course/view.php?id=99">Outra disciplina</a><a href="/course/view.php?id=1">Home</a>
                   <a href="/course/view.php?id=7">ab</a>"#,
            ),
            ("GET", "/") => Reply::text(200, "<html></html>"),
            ("GET", "/course/view.php") if s.url.ends_with("id=4235") => Reply::text(
                200,
                r#"<ul>
                <li class="section main clearfix" id="section-1"><h3 class="sectionname"><span>Sprint 2</span></h3>
                  <a href="/mod/resource/view.php?id=11">Enunciado LP3 V1.3<span> Ficheiro</span></a>
                  <a href="/mod/folder/view.php?id=22">Pasta Dados</a>
                  <a href="/mod/assign/view.php?id=33">Sprint 2 Entrega</a>
                  <a href="/mod/page/view.php?id=44">Enunciado em página</a>
                  <a href="/mod/url/view.php?id=55">Client Session</a>
                </li>
                <li class="section main clearfix" id="section-2"><a href="/mod/resource/view.php?id=12">Guia</a></li>
                </ul>"#,
            ),
            ("GET", "/course/view.php") => Reply::text(404, "não existe"),
            ("GET", "/mod/resource/view.php") if s.url.ends_with("id=11") => {
                Reply::text(302, "").with("Location", "/pluginfile.php/1/mod_resource/content/1/Enunciado%20LP3%20v1.3.pdf")
            }
            ("GET", "/mod/resource/view.php") => Reply::bytes(200, b"guia sem nome", &[("Content-Type", "application/pdf")]),
            ("GET", p) if p.starts_with("/pluginfile.php/1/") => Reply::bytes(200, &pdf2.lock().unwrap(), &[("Content-Type", "application/pdf")]),
            ("GET", "/pluginfile.php/2/mod_assign/intro/Requisitos.pdf") => Reply::bytes(200, b"%PDF requisitos", &[("Content-Type", "application/pdf")]),
            ("GET", "/mod/folder/download_folder.php") => Reply::bytes(200, &zip, &[("Content-Type", "application/zip")]),
            ("GET", "/mod/assign/view.php") => Reply::text(200, r#"<a href="/pluginfile.php/2/mod_assign/intro/Requisitos.pdf">Requisitos.pdf</a>"#),
            ("GET", "/mod/page/view.php") => Reply::text(200, r#"<html><div role="main"><h2>Enunciado</h2><p>Entrega até 8 de novembro</p></div></html>"#),
            ("GET", "/mod/url/view.php") => Reply::text(302, "").with("Location", "/externo"),
            ("GET", "/externo") => Reply::text(200, "externo"),
            _ => Reply::text(404, "?"),
        }
    });
    Moodle { srv, pdf }
}

fn engine(m: &Moodle, home: &Home, password: &str) -> MoodleSync {
    let mut e = MoodleSync::new("1234567@isep.ipp.pt", password, &home.path("dl"), None, None).unwrap();
    e.base_url = m.srv.base.clone();
    e
}

fn logged(home: &Home) -> Vec<Value> {
    std::fs::read_to_string(home.path("MoodleSync/new_files.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn sanitize_username_and_extensions() {
    assert_eq!(sanitize_filename("a<b>:\"c\"/d\\e|f?g*"), "a_b___c__d_e_f_g_");
    assert_eq!(sanitize_filename("   "), "sem_nome");
    assert_eq!(sanitize_filename(&"x".repeat(300)).chars().count(), 120);
    assert_eq!(clean_username("1234567@ISEP.ipp.pt "), "1234567");
    assert_eq!(clean_username("1234567"), "1234567");
}

fn resp(url: &str, headers: &[(&str, &str)]) -> Response {
    Response { status: 200, headers: headers.iter().map(|(k, v)| (k.to_lowercase(), v.to_string())).collect(), body: vec![], url: url.into() }
}

#[test]
fn real_name_prefers_content_disposition_then_pluginfile_url_then_label() {
    let r = resp("https://x/y", &[("Content-Disposition", "attachment; filename*=UTF-8''Enunciado%20LP3%20v1.3.pdf")]);
    assert_eq!(real_name(&r, "Enunciado LP3 V1.3Ficheiro"), "Enunciado LP3 v1.3.pdf");
    assert_eq!(real_name(&resp("https://x/y", &[("Content-Disposition", "inline; filename=\"Guia.pdf\"")]), "x"), "Guia.pdf");
    assert_eq!(real_name(&resp("https://x/pluginfile.php/1/Enunciado%20LP3.pdf?forcedownload=1", &[]), "x"), "Enunciado LP3.pdf");
    // /view.php nunca é um nome de ficheiro
    let v = resp("https://x/mod/resource/view.php?id=1", &[("Content-Type", "application/pdf")]);
    assert_eq!(real_name(&v, "Texto do link"), "Texto do link.pdf");
    let html = resp("https://x/y", &[("Content-Type", "text/html; charset=utf-8")]);
    assert_eq!(real_name(&html, "Pagina"), "Pagina");
}

#[test]
fn login_success_wrong_password_and_missing_session() {
    let home = Home::new();
    let m = fake_moodle();
    assert!(engine(&m, &home, "boa").login());
    assert!(!engine(&m, &home, "ma").login());
    // um Moodle que aceita mas nunca devolve sesskey não conta como sessão
    let bare = fake_server(|_| Reply::text(200, "<html>nada</html>"));
    let mut e = MoodleSync::new("1", "boa", &home.path("dl"), None, None).unwrap();
    e.base_url = bare.base.clone();
    assert!(!e.login());
}

#[test]
fn enrolled_courses_merge_ajax_and_scraping_without_duplicates_or_site_course() {
    let home = Home::new();
    let m = fake_moodle();
    let mut e = engine(&m, &home, "boa");
    assert!(e.login());
    let courses = e.get_enrolled_courses();
    assert_eq!(courses, vec![("4235".to_string(), LAB.to_string()), ("99".to_string(), "Outra disciplina".to_string())]);
}

#[test]
fn full_sync_downloads_everything_with_real_names_and_is_idempotent() {
    let home = Home::new();
    let m = fake_moodle();
    let mut e = engine(&m, &home, "boa");
    assert!(e.login());
    e.sync_course("4235", LAB);
    assert_eq!(e.errors, 0);

    let course = home.path("dl").join(LAB.replace('/', "_"));
    let sec1 = course.join("01 - Sprint 2");
    assert_eq!(std::fs::read(sec1.join("Enunciado LP3 v1.3.pdf")).unwrap(), b"%PDF-1.4 versao 1", "nome real vindo do URL final");
    assert_eq!(std::fs::read(sec1.join("Pasta Dados/a.txt")).unwrap(), b"ficheiro a");
    assert_eq!(std::fs::read(sec1.join("Pasta Dados/sub/b.txt")).unwrap(), b"ficheiro b");
    assert!(!course.join("evil.txt").exists() && !sec1.join("evil.txt").exists() && !home.path("dl").join("evil.txt").exists(), "zip slip");
    assert_eq!(std::fs::read(sec1.join("Trabalhos/Sprint 2 Entrega/Requisitos.pdf")).unwrap(), b"%PDF requisitos");
    let page = std::fs::read_to_string(sec1.join("Paginas/Enunciado em página.txt")).unwrap();
    assert!(page.contains("Enunciado") && page.contains("Entrega até 8 de novembro"));
    let shortcut = std::fs::read_to_string(sec1.join("Links/Client Session.url")).unwrap();
    assert!(shortcut.starts_with("[InternetShortcut]\nURL=http://127.0.0.1:") && shortcut.trim_end().ends_with("/externo"));
    assert_eq!(std::fs::read(course.join("02 - Secção 02/Guia.pdf")).unwrap(), b"guia sem nome");

    let entries = logged(&home);
    let names: Vec<&str> = entries.iter().map(|e| e["name"].as_str().unwrap()).collect();
    for n in ["Enunciado LP3 v1.3.pdf", "Requisitos.pdf", "Enunciado em página.txt", "Guia.pdf"] {
        assert!(names.contains(&n), "{n} em {names:?}");
    }
    assert!(entries.iter().all(|e| e["kind"] == "novo" && e["course"] == LAB));
    assert_eq!(entries[0]["section"], "Sprint 2");
    let first_run = entries.len();

    // segunda execução: nada mudou, nada se regista
    let mut e2 = engine(&m, &home, "boa");
    assert!(e2.login());
    e2.sync_course("4235", LAB);
    assert_eq!(logged(&home).len(), first_run);
    assert!(e2.new_files.is_empty());

    // o PDF muda no servidor: regista como atualizado
    *m.pdf.lock().unwrap() = b"%PDF-1.4 versao 2 maior".to_vec();
    let mut e3 = engine(&m, &home, "boa");
    assert!(e3.login());
    e3.sync_course("4235", LAB);
    let latest = logged(&home);
    assert_eq!(latest.len(), first_run + 1);
    assert_eq!(latest.last().unwrap()["kind"], "atualizado");
    assert_eq!(std::fs::read(sec1.join("Enunciado LP3 v1.3.pdf")).unwrap(), b"%PDF-1.4 versao 2 maior");
    assert!(!walk(&home.path("dl")).iter().any(|p| p.extension().map(|e| e == "tmp").unwrap_or(false)));
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                out.extend(walk(&e.path()));
            } else {
                out.push(e.path());
            }
        }
    }
    out
}

#[test]
fn legacy_extensionless_file_is_migrated() {
    let home = Home::new();
    let m = fake_moodle();
    let mut e = engine(&m, &home, "boa");
    let folder = home.path("dl/Lab");
    std::fs::create_dir_all(&folder).unwrap();
    let legacy = folder.join("Enunciado LP3Ficheiro");
    std::fs::write(&legacy, b"antigo").unwrap();
    assert!(e.login());
    assert!(e.download_file(&format!("{}/mod/resource/view.php?id=11", m.srv.base), &legacy));
    assert!(!legacy.exists());
    assert!(folder.join("Enunciado LP3 v1.3.pdf").exists());
}

#[test]
fn download_errors_are_counted_not_fatal() {
    let home = Home::new();
    let m = fake_moodle();
    let mut e = engine(&m, &home, "boa");
    assert!(!e.download_file(&format!("{}/naoexiste", m.srv.base), &home.path("dl/x")));
    assert_eq!(e.errors, 1);
    assert!(!e.download_file("http://127.0.0.1:1/x", &home.path("dl/y")));
    assert_eq!(e.errors, 2);
}

#[test]
fn run_all_skips_when_locked_and_releases_the_lock() {
    let home = Home::new();
    let m = fake_moodle();
    std::fs::write(home.path("MoodleSync/sync.lock"), "123").unwrap();
    let mut busy = engine(&m, &home, "boa");
    assert!(!busy.run_all() && busy.busy);
    std::fs::remove_file(home.path("MoodleSync/sync.lock")).unwrap();

    let mut e = engine(&m, &home, "boa");
    assert!(e.run_all(), "o curso 99 devolve 404 mas não é um erro de descarga");
    assert!(!home.path("MoodleSync/sync.lock").exists());
    assert!(!e.new_files.is_empty());
    let mut wrong = engine(&m, &home, "ma");
    assert!(!wrong.run_all() && !wrong.busy);
    assert!(!home.path("MoodleSync/sync.lock").exists());
}

#[cfg(windows)]
#[test]
fn run_sync_requires_credentials_then_records_history_and_marks_the_day() {
    let home = Home::new();
    let m = fake_moodle();
    let none = run_sync("manual", None, Some(&m.srv.base), None);
    assert!(!none.ok && none.error.as_deref().unwrap().contains("Configuração em falta") && history::last("moodle_sync", false).is_none());

    let user = set_credentials("1234567@isep.ipp.pt", "boa", Some(&m.srv.base), None, &home.path("dl")).unwrap();
    assert_eq!(user, "1234567");
    let stored = std::fs::read_to_string(home.path("MoodleSync/config.json")).unwrap();
    assert!(!stored.contains("\"boa\"") && stored.contains("encrypted_password") && stored.contains("\"username\": \"1234567\""));

    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = lines.clone();
    let res = run_sync("api", None, Some(&m.srv.base), Some(Box::new(move |l| sink.lock().unwrap().push(l.to_string()))));
    assert!(res.ok, "{:?} {:?}", res.error, lines.lock().unwrap());
    assert!(res.new_files >= 4 && res.errors == 0);
    assert!(lines.lock().unwrap().iter().any(|l| l.contains("Autenticação com sucesso")));
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert_eq!(load_sync_config(None)["last_sync_date"], today);

    let rec = history::last("moodle_sync", false).unwrap();
    assert_eq!(rec["trigger"], "api");
    assert!(rec["details"]["new_files"].as_u64().unwrap() >= 4);
    assert!(!rec.to_string().contains("boa"), "a palavra-passe nunca vai para o histórico");
    let log = std::fs::read_to_string(home.path("MoodleSync/sync_log.txt")).unwrap();
    assert!(!log.contains("password=") && !log.contains("=boa"));
}

#[cfg(windows)]
#[test]
fn run_sync_failure_is_recorded_and_does_not_mark_the_day() {
    let home = Home::new();
    let m = fake_moodle();
    set_credentials("1234567", "boa", Some(&m.srv.base), None, &home.path("dl")).unwrap();
    // a palavra-passe guardada deixa de ser aceite
    let mut cfg = load_sync_config(None);
    cfg.insert("encrypted_password".into(), json!(isep_assistant::dpapi::encrypt("ma").unwrap()));
    save_sync_config(None, &cfg).unwrap();
    let res = run_sync("scheduled", None, Some(&m.srv.base), None);
    assert!(!res.ok && res.error.is_some());
    assert_eq!(history::last("moodle_sync", false).unwrap()["ok"], false);
    assert_eq!(load_sync_config(None)["last_sync_date"], "");
}

#[cfg(windows)]
#[test]
fn set_credentials_rejects_bad_login_without_touching_the_config() {
    let home = Home::new();
    let m = fake_moodle();
    let err = set_credentials("1234567", "ma", Some(&m.srv.base), None, &home.path("dl")).unwrap_err();
    assert!(err.contains("recusou o login"));
    assert!(!home.path("MoodleSync/config.json").exists());
    assert!(set_credentials("", "x", Some(&m.srv.base), None, &home.path("dl")).is_err());
}
