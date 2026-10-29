mod common;
use chrono::Local;
use common::*;
use isep_assistant::api::{handle, App, Req, Resp};
use isep_assistant::config::{load_config, save_config};
use isep_assistant::model::Email;
use isep_assistant::rules::Rules;
use isep_assistant::summarizer::summarize_emails;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const TOKEN: &str = "token-de-teste";

fn call(method: &str, path: &str, body: Option<Value>) -> Resp {
    let (p, query) = match path.split_once('?') {
        Some((p, q)) => (p, q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()),
        None => (path, vec![]),
    };
    handle(
        &App::new(TOKEN),
        &Req {
            method: method.into(),
            path: format!("/api/v1{p}"),
            query,
            host: Some("127.0.0.1:1".into()),
            authorization: Some(format!("Bearer {TOKEN}")),
            body: body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
        },
    )
}

fn repo_script(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("EmailNotifier").join(name)
}

fn node() -> Option<std::path::PathBuf> {
    isep_assistant::util::which("node")
}

// ---------------------------------------------------------------- servidor IMAP falso

/// Servidor IMAP mínimo (sem TLS): saudação, XOAUTH2, SELECT, UID SEARCH e UID FETCH. Guarda o que o cliente enviou.
fn fake_imap(messages: Vec<(u32, String)>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else { return };
        let mut out = stream.try_clone().unwrap();
        let reader = BufReader::new(stream);
        out.write_all(b"* OK IMAP pronto\r\n").unwrap();
        for line in reader.lines().map_while(Result::ok) {
            seen2.lock().unwrap().push(line.clone());
            let (tag, cmd) = line.split_once(' ').unwrap_or((&line, ""));
            if cmd.starts_with("AUTHENTICATE XOAUTH2") {
                out.write_all(format!("{tag} OK autenticado\r\n").as_bytes()).unwrap();
            } else if cmd.starts_with("SELECT") {
                out.write_all(format!("* {} EXISTS\r\n{tag} OK [READ-WRITE] aberta\r\n", messages.len()).as_bytes()).unwrap();
            } else if cmd.starts_with("UID SEARCH") {
                let uids: Vec<String> = messages.iter().map(|(u, _)| u.to_string()).collect();
                out.write_all(format!("* SEARCH {}\r\n{tag} OK feito\r\n", uids.join(" ")).as_bytes()).unwrap();
            } else if cmd.starts_with("UID FETCH") {
                for (i, (uid, raw)) in messages.iter().enumerate() {
                    let head = format!("* {} FETCH (UID {uid} BODY[] {{{}}}\r\n", i + 1, raw.len());
                    out.write_all(head.as_bytes()).unwrap();
                    out.write_all(raw.as_bytes()).unwrap();
                    out.write_all(b")\r\n").unwrap();
                }
                out.write_all(format!("{tag} OK feito\r\n").as_bytes()).unwrap();
            } else if cmd.starts_with("LOGOUT") {
                out.write_all(format!("* BYE\r\n{tag} OK adeus\r\n").as_bytes()).unwrap();
                break;
            }
        }
    });
    (port, seen)
}

fn raw_mail(subject: &str, id: &str, body: &str) -> String {
    let date = Local::now().format("%a, %d %b %Y %H:%M:%S +0000").to_string();
    format!("From: Docente <docente@escola.pt>\r\nSubject: {subject}\r\nDate: {date}\r\nMessage-ID: <{id}>\r\n\r\n{body}\r\n")
}

#[cfg(windows)]
#[test]
fn own_email_service_reads_the_inbox_over_imap_with_oauth_and_no_external_client() {
    let Some(node) = node() else { return }; // sem Node na máquina de desenvolvimento não há como correr o leitor
    let _home = Home::new();
    let login = fake_server(|_| Reply::json(200, json!({"access_token": "at-secreto", "refresh_token": "rt-novo"})));
    let (port, seen) = fake_imap(vec![(101, raw_mail("Prazo do PI", "i1@escola", "Entrega até sexta.")), (102, raw_mail("Aviso", "i2@escola", "Aula cancelada."))]);
    std::env::set_var("ISEP_MS_LOGIN_URL", &login.base);
    std::env::set_var("ISEP_IMAP_HOST", "127.0.0.1");
    std::env::set_var("ISEP_IMAP_PORT", port.to_string());
    std::env::set_var("ISEP_IMAP_PLAIN", "1");
    std::env::set_var("ISEP_NODE", &node);
    std::env::set_var("ISEP_MAIL_SCRIPT", repo_script("mail_fetcher.cjs"));
    let mut cfg = json_map(json!({"email_method": "imap", "email_address": "aluno@escola.pt"}));
    isep_assistant::oauth::store_refresh_token(&mut cfg, "rt-velho").unwrap();
    save_config(&cfg).unwrap();

    let r = call("GET", "/emails?days=3", None);
    for v in ["ISEP_MS_LOGIN_URL", "ISEP_IMAP_HOST", "ISEP_IMAP_PORT", "ISEP_IMAP_PLAIN", "ISEP_NODE", "ISEP_MAIL_SCRIPT"] {
        std::env::remove_var(v);
    }
    assert_eq!(r.status, 200);
    assert_eq!(r.body["error"], Value::Null, "{}", r.body);
    assert_eq!(r.body["source"], "imap");
    assert_eq!(r.body["total"], 2);
    let subjects: Vec<&str> = r.body["items"].as_array().unwrap().iter().map(|m| m["subject"].as_str().unwrap()).collect();
    assert!(subjects.contains(&"Prazo do PI") && subjects.contains(&"Aviso"), "{subjects:?}");
    assert!(r.body["items"][0]["preview"].as_str().unwrap().len() > 3);

    // o token foi enviado ao servidor no formato XOAUTH2 e o de renovação novo ficou guardado cifrado
    let auth = seen.lock().unwrap().iter().find(|l| l.contains("AUTHENTICATE XOAUTH2")).cloned().unwrap();
    let b64 = auth.rsplit(' ').next().unwrap();
    let decoded = String::from_utf8_lossy(&decode_b64(b64)).into_owned();
    assert!(decoded.contains("user=aluno@escola.pt") && decoded.contains("auth=Bearer at-secreto"), "{decoded:?}");
    assert_eq!(isep_assistant::dpapi::decrypt(load_config()["ms_refresh_token"].as_str().unwrap()).unwrap(), "rt-novo");
    assert!(seen.lock().unwrap().iter().any(|l| l.contains("UID SEARCH SINCE")));
}

fn decode_b64(s: &str) -> Vec<u8> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.bytes().filter(|c| *c != b'=') {
        acc = (acc << 6) | T.iter().position(|t| *t == c).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

#[cfg(windows)]
#[test]
fn imap_wrong_authorization_gives_a_readable_error() {
    let Some(node) = node() else { return };
    let _home = Home::new();
    let login = fake_server(|_| Reply::json(200, json!({"access_token": "at", "refresh_token": "rt"})));
    // servidor que recusa a autenticação
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut out = stream.try_clone().unwrap();
        out.write_all(b"* OK pronto\r\n").unwrap();
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let tag = line.split(' ').next().unwrap().to_string();
            if line.contains("AUTHENTICATE") {
                out.write_all(format!("{tag} NO AUTHENTICATE failed.\r\n").as_bytes()).unwrap();
                break;
            }
        }
    });
    std::env::set_var("ISEP_MS_LOGIN_URL", &login.base);
    std::env::set_var("ISEP_IMAP_HOST", "127.0.0.1");
    std::env::set_var("ISEP_IMAP_PORT", port.to_string());
    std::env::set_var("ISEP_IMAP_PLAIN", "1");
    std::env::set_var("ISEP_NODE", &node);
    std::env::set_var("ISEP_MAIL_SCRIPT", repo_script("mail_fetcher.cjs"));
    let mut cfg = json_map(json!({"email_method": "imap", "email_address": "aluno@escola.pt"}));
    isep_assistant::oauth::store_refresh_token(&mut cfg, "rt").unwrap();
    save_config(&cfg).unwrap();
    let r = call("GET", "/emails", None);
    for v in ["ISEP_MS_LOGIN_URL", "ISEP_IMAP_HOST", "ISEP_IMAP_PORT", "ISEP_IMAP_PLAIN", "ISEP_NODE", "ISEP_MAIL_SCRIPT"] {
        std::env::remove_var(v);
    }
    assert!(r.body["error"].as_str().unwrap().contains("recusou o início de sessão"), "{}", r.body);
}

#[test]
fn imap_needs_the_students_email_address() {
    let _home = Home::new();
    save_config(&json_map(json!({"email_method": "imap"}))).unwrap();
    assert!(call("GET", "/emails", None).body["error"].as_str().unwrap().contains("email da escola"));
}

// ---------------------------------------------------------------- regras do resumo

fn mails() -> Vec<Email> {
    vec![
        Email::new("Promoção do bar", "Bar <bar@escola.pt>", "Menu da semana."),
        Email::new("Teste de LAPR3 adiado", "Docente <docente@escola.pt>", "O teste passa para sexta."),
        Email::new("Newsletter", "Marketing <marketing@escola.pt>", "Novidades."),
    ]
}

#[test]
fn rules_hide_ignored_senders_and_subjects_and_put_important_emails_first() {
    let cfg = json_map(json!({"summary_rules": {"important_keywords": ["teste"], "ignore_senders": ["marketing@"], "ignore_subjects": ["promoção"]}}));
    let out = summarize_emails(&mails(), &cfg, &[]);
    assert!(!out.contains("Newsletter") && !out.contains("Promoção"), "{out}");
    let important = out.find("⭐ IMPORTANTE").expect("secção importante");
    assert!(out[important..].contains("Teste de LAPR3 adiado"));
    assert!(out.contains("1 emails novos"), "{out}");
    // sem regras, tudo aparece
    let all = summarize_emails(&mails(), &json_map(json!({})), &[]);
    assert!(all.contains("Newsletter") && all.contains("Promoção") && !all.contains("IMPORTANTE"));
}

#[test]
fn everything_ignored_means_nothing_to_send() {
    let cfg = json_map(json!({"summary_rules": {"ignore_senders": ["escola.pt"]}}));
    assert!(summarize_emails(&mails(), &cfg, &[]).contains("sem emails novos"));
}

#[test]
fn rules_are_validated_before_they_are_saved() {
    let _home = Home::new();
    let ok = call("PUT", "/settings", Some(json!({"summary": {"rules": {"important_keywords": ["  prazo  ", "Prazo", "", "teste final"], "ignore_senders": ["noreply@x.pt"]}}})));
    assert_eq!(ok.status, 200, "{}", ok.body);
    let rules = &call("GET", "/settings", None).body["summary"]["rules"];
    assert_eq!(rules["important_keywords"], json!(["prazo", "teste final"]), "limpa espaços, linhas em branco e repetidos");
    assert_eq!(rules["ignore_senders"], json!(["noreply@x.pt"]));

    for (bad, field) in [
        (json!({"important_keywords": ["a"]}), "summary.rules.important_keywords"),
        (json!({"ignore_senders": ["x".repeat(61)]}), "summary.rules.ignore_senders"),
        (json!({"ignore_subjects": "texto solto"}), "summary.rules.ignore_subjects"),
        (json!({"ignore_subjects": [1, 2]}), "summary.rules.ignore_subjects"),
        (json!({"important_keywords": (0..31).map(|i| format!("palavra{i}")).collect::<Vec<_>>()}), "summary.rules.important_keywords"),
        (json!({"importante": ["x"]}), "summary.rules.importante"),
    ] {
        let r = call("PUT", "/settings", Some(json!({"summary": {"rules": bad}})));
        assert_eq!(r.status, 422, "{bad}");
        assert!(r.body["errors"].get(field).is_some(), "{field}: {}", r.body);
    }
    // um pedido inválido não altera o que estava guardado
    assert_eq!(call("GET", "/settings", None).body["summary"]["rules"]["important_keywords"], json!(["prazo", "teste final"]));
    assert!(Rules::from_config(&load_config()).important_keywords.contains(&"prazo".to_string()));
}

// ---------------------------------------------------------------- perfil do estudante e Moodle

#[test]
fn student_profile_is_validated_and_kept_locally() {
    let _home = Home::new();
    assert_eq!(call("GET", "/settings", None).body["student"]["configured"], false);
    let ok = call("PUT", "/settings", Some(json!({"student": {"institution_id": "isep", "course": "Licenciatura em Engenharia Informática", "year": 3}})));
    assert_eq!(ok.status, 200, "{}", ok.body);
    let s = call("GET", "/settings", None).body["student"].clone();
    assert_eq!(s["institution_id"], "isep");
    assert_eq!(s["institution_name"], "ISEP – Instituto Superior de Engenharia do Porto (P.PORTO)");
    assert_eq!(s["year"], 3);
    assert_eq!(s["configured"], true);

    for (bad, field) in [
        (json!({"institution_id": "nao-existe"}), "student.institution_id"),
        (json!({"course": "x"}), "student.course"),
        (json!({"year": 0}), "student.year"),
        (json!({"year": 7}), "student.year"),
        (json!({"year": "terceiro"}), "student.year"),
    ] {
        let r = call("PUT", "/settings", Some(json!({"student": bad})));
        assert_eq!(r.status, 422, "{bad}");
        assert!(r.body["errors"].get(field).is_some(), "{field}: {}", r.body);
    }
    // «outra» exige o nome da instituição
    let r = call("PUT", "/settings", Some(json!({"student": {"institution_id": "outra"}})));
    assert_eq!(r.status, 422);
    assert!(r.body["errors"].get("student.institution_name").is_some());
    let ok = call("PUT", "/settings", Some(json!({"student": {"institution_id": "outra", "institution_name": "Escola Nova"}})));
    assert_eq!(ok.status, 200);
    assert_eq!(call("GET", "/settings", None).body["student"]["institution_name"], "Escola Nova");
}

#[test]
fn institutions_catalog_marks_which_have_a_known_moodle() {
    let _home = Home::new();
    let items = call("GET", "/institutions", None).body["items"].clone();
    let items = items.as_array().unwrap();
    assert!(items.len() > 30);
    let find = |id: &str| items.iter().find(|i| i["id"] == id).unwrap().clone();
    assert_eq!(find("isep")["has_moodle"], true);
    assert_eq!(find("uminho")["has_moodle"], false);
    let ids: std::collections::HashSet<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), items.len(), "ids repetidos");
}

#[test]
fn moodle_address_comes_from_the_institution_or_is_asked_for() {
    let _home = Home::new();
    // instalação antiga, sem perfil: continua no Moodle do ISEP
    let m = call("GET", "/settings", None).body["moodle"].clone();
    assert_eq!(m["url"], "https://moodle.isep.ipp.pt");
    assert_eq!(m["url_source"], "default");

    call("PUT", "/settings", Some(json!({"student": {"institution_id": "ubi", "course": "Engenharia Informática", "year": 1}})));
    let m = call("GET", "/settings", None).body["moodle"].clone();
    assert_eq!((m["url"].as_str(), m["url_source"].as_str()), (Some("https://moodle.ubi.pt"), Some("institution")));

    // instituição sem Moodle conhecido: falta o endereço
    call("PUT", "/settings", Some(json!({"student": {"institution_id": "uminho"}})));
    let m = call("GET", "/settings", None).body["moodle"].clone();
    assert_eq!((m["url"].clone(), m["url_source"].as_str()), (Value::Null, Some("missing")));
    let sync = call("POST", "/moodle/sync", None);
    assert_eq!(sync.status, 202);

    // o utilizador indica-o (normaliza-se) e passa a ser esse
    let r = call("PUT", "/settings", Some(json!({"moodle": {"url": "  moodle.uminho.pt/login/index.php "}})));
    assert_eq!(r.status, 200, "{}", r.body);
    let m = call("GET", "/settings", None).body["moodle"].clone();
    assert_eq!((m["url"].as_str(), m["url_source"].as_str()), (Some("https://moodle.uminho.pt"), Some("custom")));
    assert_eq!(call("PUT", "/settings", Some(json!({"moodle": {"url": "isto não é um endereço"}}))).status, 422);
    // vazio volta ao da instituição
    call("PUT", "/settings", Some(json!({"moodle": {"url": ""}})));
    assert_eq!(call("GET", "/settings", None).body["moodle"]["url_source"], "missing");
}

#[test]
fn probe_accepts_a_moodle_and_rejects_anything_else() {
    let _home = Home::new();
    let moodle = fake_server(|_| Reply::text(200, "<html><title>Entrar</title><body class='moodle'>Moodle login</body></html>"));
    let other = fake_server(|_| Reply::text(200, "<html>Outro site</html>"));
    let missing = fake_server(|_| Reply::text(404, "nada"));
    let ok = call("POST", "/moodle/probe", Some(json!({"url": format!("{}/", moodle.base)})));
    assert_eq!(ok.status, 200, "{}", ok.body);
    assert_eq!(ok.body["url"], moodle.base);
    assert_eq!(call("POST", "/moodle/probe", Some(json!({"url": other.base}))).status, 422);
    assert_eq!(call("POST", "/moodle/probe", Some(json!({"url": missing.base}))).status, 422);
    assert_eq!(call("POST", "/moodle/probe", Some(json!({"url": ""}))).status, 422);
    assert_eq!(call("POST", "/moodle/probe", Some(json!({"url": "http://127.0.0.1:1"}))).status, 422, "sem servidor");
}

#[test]
fn download_folder_defaults_can_be_changed_reset_and_auto_download_switched_off() {
    let home = Home::new();
    let m = call("GET", "/settings", None).body["moodle"].clone();
    assert_eq!(m["download_dir_is_default"], true);
    assert_eq!(m["auto_download"], true);
    assert_eq!(m["download_dir"], m["default_download_dir"]);

    let custom = home.path("meus-downloads");
    call("PUT", "/settings", Some(json!({"moodle": {"download_dir": custom.to_string_lossy(), "auto_download": false}})));
    let m = call("GET", "/settings", None).body["moodle"].clone();
    assert_eq!(m["download_dir_is_default"], false);
    assert_eq!(m["auto_download"], false);
    assert_eq!(m["download_dir"], custom.to_string_lossy().as_ref());

    // vazio repõe a pasta predefinida
    assert_eq!(call("PUT", "/settings", Some(json!({"moodle": {"download_dir": ""}}))).status, 200);
    assert_eq!(call("GET", "/settings", None).body["moodle"]["download_dir_is_default"], true);
    assert_eq!(call("PUT", "/settings", Some(json!({"moodle": {"auto_download": "sim"}}))).status, 422);
}

#[test]
fn auto_download_off_skips_the_sync_before_the_summary() {
    let home = Home::new();
    std::fs::write(home.path("MoodleSync/config.json"), "{}").unwrap();
    let cfg = json_map(json!({"moodle_auto_download": false}));
    let logged = Mutex::new(Vec::<String>::new());
    // com o descarregamento desativado nem sequer tenta (não há credenciais nem rede, e não deve falhar nem esperar)
    isep_assistant::digest::ensure_moodle_synced(&cfg, &|l| logged.lock().unwrap().push(l.to_string()));
    assert!(logged.lock().unwrap().is_empty());
    assert!(!home.path("MoodleSync/sync_progress.json").exists());
}

// ---------------------------------------------------------------- Session: conta nova

#[cfg(windows)]
#[test]
fn creating_a_session_account_returns_the_recovery_phrase_once_and_stores_only_the_id() {
    let Some(node) = node() else { return };
    let _home = Home::new();
    std::env::set_var("ISEP_NODE", &node);
    std::env::set_var("ISEP_SESSION_SCRIPT", repo_script("session_sender.cjs"));
    let r = call("POST", "/session/account", None);
    std::env::remove_var("ISEP_NODE");
    std::env::remove_var("ISEP_SESSION_SCRIPT");
    assert_eq!(r.status, 200, "{}", r.body);
    let (id, phrase) = (r.body["session_id"].as_str().unwrap(), r.body["mnemonic"].as_str().unwrap());
    assert!(id.starts_with("05") && id.len() == 66);
    assert_eq!(phrase.split_whitespace().count(), 13);
    let saved = std::fs::read_to_string(isep_assistant::paths::email_config()).unwrap();
    assert!(!saved.contains(phrase), "a frase de recuperação do utilizador nunca fica guardada");
    assert!(saved.contains(id), "o ID fica como destinatário");
    assert!(!call("GET", "/session", None).body.to_string().contains(phrase));
}
