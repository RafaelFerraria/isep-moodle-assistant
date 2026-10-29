mod common;
use chrono::{Local, NaiveDate, TimeZone};
use common::*;
use isep_assistant::{dpapi, mail, session, system};
use serde_json::json;
use std::cell::RefCell;

fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 0).unwrap()
}

/// Hora local equivalente a uma hora UTC.
fn utc_to_local(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::NaiveDateTime {
    chrono::Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap().with_timezone(&Local).naive_local()
}

const MBOX: &str = "From - Mon Oct 26 18:53:04 2026\n\
Subject: =?UTF-8?B?U0VNXzNfUEkgMjAyNi8yMDI3OiBBUlFDUCAtIGZ1bsOnw7VlcyBwb3IgZ3J1cG8=?=\n\
From: \"SILVA 1234567 (via MoodleISEP)\" <1234567@isep.ipp.pt>\n\
Date: Mon, 26 Oct 2026 18:53:04 +0000\n\
Message-ID: <abc123@moodle.isep.ipp.pt>\n\
Content-Type: text/plain; charset=UTF-8\n\
\n\
SEM_3_PI 2026/2027<https://moodle.isep.ipp.pt/course/view.php?id=4838> » Fóruns<https://x> » ARQCP<https://y> » ARQCP - funções por grupo<https://z>\n\
\n\
>From aqui começa uma linha escapada do mbox\n\
\n\
From - Mon Oct 26 11:04:00 2026\n\
Subject: [ISEP - Estudantes] Ementa Semanal - Bar =?iso-8859-1?Q?edif=EDcio?= E\n\
From: Secretariado ISEP <secretariado@isep.ipp.pt>\n\
Date: Mon, 26 Oct 2026 11:04:00 +0000\n\
Message-ID: <ementa1@isep.ipp.pt>\n\
MIME-Version: 1.0\n\
Content-Type: text/plain; charset=iso-8859-1\n\
Content-Transfer-Encoding: quoted-printable\n\
\n\
Segue em anexo, informa=E7=E3o da ementa do Bar do edif=EDcio E.\n\
\n\
From - Mon Oct 26 09:00:00 2026\n\
Subject: Aviso em HTML\n\
From: docente@isep.ipp.pt\n\
Date: Mon, 26 Oct 2026 09:00:00 +0000\n\
Message-ID: <html1@isep.ipp.pt>\n\
MIME-Version: 1.0\n\
Content-Type: multipart/alternative; boundary=\"BND\"\n\
\n\
--BND\n\
Content-Type: text/plain; charset=utf-8\n\
\n\
Texto simples do aviso.\n\
--BND\n\
Content-Type: text/html; charset=utf-8\n\
\n\
<p>Texto <b>HTML</b> do aviso.</p>\n\
--BND--\n\
\n\
From - Sat Oct 10 09:00:00 2026\n\
Subject: Muito antigo\n\
From: velho@isep.ipp.pt\n\
Date: Sat, 10 Oct 2026 09:00:00 +0000\n\
Message-ID: <velho@isep.ipp.pt>\n\
\n\
Já não interessa.\n";

fn write_mbox(home: &Home) -> std::path::PathBuf {
    let p = home.path("Profiles/abc.default/ImapMail/outlook.office365.com/INBOX");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, MBOX).unwrap();
    p
}

#[test]
fn mbox_is_parsed_with_charsets_encoded_headers_and_date_filter() {
    let home = Home::new();
    let path = write_mbox(&home);
    let emails = mail::read_mbox(&path, local(2026, 10, 20, 0, 0)).unwrap();
    assert_eq!(emails.len(), 3, "o email antigo tem de ficar de fora");
    // mais recentes primeiro
    assert_eq!(emails[0].subject, "SEM_3_PI 2026/2027: ARQCP - funções por grupo");
    assert_eq!(emails[0].message_id, "<abc123@moodle.isep.ipp.pt>");
    assert!(emails[0].sender.contains("SILVA 1234567") && emails[0].sender.contains("1234567@isep.ipp.pt"), "{}", emails[0].sender);
    assert!(emails[0].body.contains("» Fóruns") && emails[0].body.contains("From aqui começa"), "{}", emails[0].body);
    assert_eq!(emails[0].received_time, utc_to_local(2026, 10, 26, 18, 53, 4).format("%Y-%m-%dT%H:%M:%S").to_string());

    assert_eq!(emails[1].subject, "[ISEP - Estudantes] Ementa Semanal - Bar edifício E");
    assert!(emails[1].body.contains("informação da ementa do Bar do edifício E"), "{}", emails[1].body);

    assert_eq!(emails[2].subject, "Aviso em HTML");
    assert!(emails[2].body.contains("Texto simples do aviso."));
    assert_eq!(emails[2].sender, "docente@isep.ipp.pt");
}

#[test]
fn mbox_since_in_the_future_and_missing_file() {
    let home = Home::new();
    let path = write_mbox(&home);
    assert!(mail::read_mbox(&path, local(2030, 1, 1, 0, 0)).unwrap().is_empty());
    assert!(mail::read_mbox(&home.path("naoexiste"), local(2020, 1, 1, 0, 0)).is_err());
}

#[test]
fn body_is_limited_and_empty_subject_gets_a_default() {
    let home = Home::new();
    let big = "x".repeat(9000);
    let mbox = format!("From - x\nFrom: a@b.pt\nDate: Mon, 26 Oct 2026 09:00:00 +0000\nMessage-ID: <1@x>\n\n{big}\n");
    let p = home.path("INBOX");
    std::fs::write(&p, mbox).unwrap();
    let e = mail::read_mbox(&p, local(2026, 1, 1, 0, 0)).unwrap();
    assert_eq!(e[0].subject, "Sem assunto");
    assert!(e[0].body.chars().count() <= 4000);
}

#[test]
fn inboxes_are_found_under_the_profiles_dir_and_get_emails_reads_the_newest() {
    let home = Home::new();
    write_mbox(&home);
    std::env::set_var("ISEP_THUNDERBIRD_PROFILES", home.path("Profiles"));
    let found = mail::find_thunderbird_inboxes();
    let emails = mail::get_emails(&json_map(json!({"email_method": "thunderbird"})), local(2026, 10, 20, 0, 0), mail::FetchOptions { sync: false });
    std::env::remove_var("ISEP_THUNDERBIRD_PROFILES");
    assert_eq!(found.len(), 1);
    assert_eq!(emails.unwrap().len(), 3);
}

#[test]
fn missing_inbox_and_unsupported_method_give_clear_errors() {
    let home = Home::new();
    std::env::set_var("ISEP_THUNDERBIRD_PROFILES", home.path("vazio"));
    let none = mail::get_emails(&json_map(json!({"email_method": "thunderbird"})), local(2026, 1, 1, 0, 0), mail::FetchOptions { sync: false });
    std::env::remove_var("ISEP_THUNDERBIRD_PROFILES");
    assert!(none.unwrap_err().contains("Caixa de entrada do Thunderbird não encontrada"));
    let other = mail::get_emails(&json_map(json!({"email_method": "pop3"})), local(2026, 1, 1, 0, 0), mail::FetchOptions { sync: false });
    assert!(other.unwrap_err().contains("ainda não está disponível"));
}

// ---------------------------------------------------------------- sistema

#[test]
fn parse_tasks_json_accepts_list_and_single_object_and_hides_never() {
    let one = json!({"name": "T", "state": "Ready", "last_run": "1999-11-30T00:00:00", "next_run": "2026-10-27T18:00:00", "last_result": 0});
    let parsed = system::parse_tasks_json(&one.to_string());
    assert_eq!(parsed, vec![json!({"name": "T", "state": "Ready", "last_run": null, "next_run": "2026-10-27T18:00:00", "last_result": 0, "target": null, "target_ok": null})]);
    assert_eq!(system::parse_tasks_json(&json!([one, one]).to_string()).len(), 2);
    assert!(system::parse_tasks_json("").is_empty());
    assert!(system::parse_tasks_json("lixo").is_empty());
}

#[test]
fn registration_script_has_time_runner_and_both_tasks() {
    let s = system::registration_script("09:30", "C:\\app\\isep-assistant-api.exe", "digest --root \"C:\\dados\"");
    assert!(s.contains("-At \"09:30\""));
    assert!(s.contains("-Execute \"C:\\app\\isep-assistant-api.exe\""));
    assert!(s.contains("digest --root"));
    assert!(s.contains("ISEP_Email_Session_Daily") && s.contains("ISEP_Email_Session_OnLogon"));
    assert!(s.contains("Power-Troubleshooter"));
}

#[test]
fn query_tasks_runs_without_crashing() {
    // lê o Agendador real do Windows (só leitura): o resultado depende da máquina, mas nunca deve falhar
    for t in system::query_tasks() {
        assert!(t["name"].as_str().unwrap().starts_with("ISEP_Email_Session"));
    }
}

#[cfg(windows)]
#[test]
fn dpapi_roundtrip_and_hex_format() {
    let enc = dpapi::encrypt("palavra-passe com ç e espaços").unwrap();
    assert!(enc.len() > 100 && enc.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(!enc.contains("palavra"));
    assert_eq!(dpapi::decrypt(&enc).unwrap(), "palavra-passe com ç e espaços");
    assert_ne!(dpapi::encrypt("x").unwrap(), dpapi::encrypt("x").unwrap(), "o blob inclui sal");
    assert!(dpapi::decrypt("zz-não-hex").is_err());
    assert!(dpapi::decrypt("00ff").is_err());
}

// ---------------------------------------------------------------- Session (com um "node" falso)

#[cfg(windows)]
fn fake_node(home: &Home, exit_code: i32) -> std::path::PathBuf {
    let bin = home.path("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("node.cmd"), "@echo off\r\necho %* > \"%FAKE_NODE_OUT%\"\r\nexit /b %FAKE_NODE_EXIT%\r\n").unwrap();
    let out = home.path("node_args.txt");
    std::env::set_var("FAKE_NODE_OUT", &out);
    std::env::set_var("FAKE_NODE_EXIT", exit_code.to_string());
    let path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{};{}", bin.display(), path));
    out
}

#[cfg(windows)]
#[test]
fn session_send_passes_the_right_arguments_and_cleans_up() {
    let home = Home::new();
    let original_path = std::env::var("PATH").unwrap_or_default();
    let out = fake_node(&home, 0);
    let sid = format!("05{}", "ab".repeat(32));
    let cfg = json_map(json!({"session_recipients": [sid], "session_mnemonic": "um dois tres"}));
    let logs = RefCell::new(Vec::<String>::new());
    let ok = session::send_to_session("olá resumo", &cfg, &|l| logs.borrow_mut().push(l.to_string()));
    std::env::set_var("PATH", original_path);
    assert!(ok, "{:?}", logs.borrow());
    let args = std::fs::read_to_string(out).unwrap();
    assert!(args.contains("--to") && args.contains(&sid) && args.contains("--file") && args.contains("--mnemonic") && args.contains("um dois tres"), "{args}");
    assert!(logs.borrow().iter().any(|l| l.contains("enviada com sucesso")));
    assert!(logs.borrow().iter().all(|l| !l.contains("um dois tres")), "a mnemónica nunca vai para o log");
    let leftovers: Vec<_> = std::fs::read_dir(home.path("EmailNotifier")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("_msg_")).collect();
    assert!(leftovers.is_empty(), "o ficheiro temporário tem de ser apagado");
}

#[cfg(windows)]
#[test]
fn session_send_reports_failure_without_leaking_the_mnemonic() {
    let home = Home::new();
    let original_path = std::env::var("PATH").unwrap_or_default();
    fake_node(&home, 2);
    let sid = format!("05{}", "cd".repeat(32));
    let cfg = json_map(json!({"session_recipients": [sid], "session_mnemonic": "segredo um dois"}));
    let logs = RefCell::new(Vec::<String>::new());
    let ok = session::send_to_session("x", &cfg, &|l| logs.borrow_mut().push(l.to_string()));
    std::env::set_var("PATH", original_path);
    assert!(!ok);
    assert!(logs.borrow().iter().any(|l| l.contains("FALHA")));
    assert!(logs.borrow().iter().all(|l| !l.contains("segredo um dois")));
}

#[test]
fn session_send_needs_recipients_and_mnemonic() {
    let _home = Home::new();
    let logs = RefCell::new(Vec::<String>::new());
    assert!(!session::send_to_session("x", &json_map(json!({})), &|l| logs.borrow_mut().push(l.to_string())));
    assert!(logs.borrow()[0].contains("Nenhum destinatário"));
}
