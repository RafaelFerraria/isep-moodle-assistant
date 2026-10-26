mod common;
use common::*;
use isep_assistant::http::{decode_component, encode_component, join_url, parse_url, HttpClient, UrlParts};
use serde_json::json;
use std::time::Duration;

fn client() -> HttpClient {
    HttpClient::new(Duration::from_secs(10)).unwrap()
}

#[test]
fn parse_url_handles_ports_paths_and_fragments() {
    assert_eq!(
        parse_url("https://moodle.isep.ipp.pt/course/view.php?id=5#x").unwrap(),
        UrlParts { secure: true, host: "moodle.isep.ipp.pt".into(), port: 443, path_and_query: "/course/view.php?id=5".into() }
    );
    let p = parse_url("http://127.0.0.1:8765").unwrap();
    assert_eq!((p.secure, p.port, p.path_and_query.as_str()), (false, 8765, "/"));
    assert_eq!(parse_url("http://user:pw@host/x").unwrap().host, "host");
    assert!(parse_url("ftp://x").is_err());
    assert!(parse_url("https://").is_err());
}

#[test]
fn join_url_resolves_relative_links() {
    let base = "https://moodle.isep.ipp.pt/course/view.php?id=5";
    assert_eq!(join_url(base, "/mod/resource/view.php?id=1"), "https://moodle.isep.ipp.pt/mod/resource/view.php?id=1");
    assert_eq!(join_url(base, "edit.php"), "https://moodle.isep.ipp.pt/course/edit.php");
    assert_eq!(join_url(base, "https://outro.pt/a"), "https://outro.pt/a");
    assert_eq!(join_url("http://h:81/a/b", "/c"), "http://h:81/c");
}

#[test]
fn percent_encoding_roundtrip() {
    assert_eq!(encode_component("a b&ç"), "a%20b%26%C3%A7");
    assert_eq!(decode_component("Enunciado%20LP3%20v1.3.pdf"), "Enunciado LP3 v1.3.pdf");
}

#[test]
fn get_returns_status_headers_body_and_final_url() {
    let srv = fake_server(|_| Reply::bytes(200, b"ola", &[("X-Teste", "valor"), ("Content-Type", "text/plain")]));
    let r = client().get(&format!("{}/a/b?c=1", srv.base), &[("X-Pedido", "1")]).unwrap();
    assert_eq!(r.status, 200);
    assert_eq!(r.text(), "ola");
    assert_eq!(r.header("x-teste"), Some("valor"));
    assert_eq!(r.header("X-TESTE"), Some("valor"));
    assert!(r.url.ends_with("/a/b?c=1"));
    let seen = &srv.requests()[0];
    assert_eq!(seen.method, "GET");
    assert_eq!(seen.header("x-pedido"), Some("1"));
}

#[test]
fn post_json_and_form_send_the_right_body_and_content_type() {
    let srv = fake_server(|_| Reply::json(201, json!({"ok": true})));
    let c = client();
    let r = c.post_json(&format!("{}/api", srv.base), &[("Authorization", "Bearer k")], &json!({"a": "ç"})).unwrap();
    assert_eq!(r.status, 201);
    let r2 = c.post_form(&format!("{}/login", srv.base), &[("username", "123"), ("password", "p&ss w")]).unwrap();
    assert_eq!(r2.status, 201);
    let reqs = srv.requests();
    assert_eq!(reqs[0].header("authorization"), Some("Bearer k"));
    assert_eq!(reqs[0].header("content-type"), Some("application/json"));
    assert_eq!(reqs[0].body, r#"{"a":"ç"}"#);
    assert_eq!(reqs[1].header("content-type"), Some("application/x-www-form-urlencoded"));
    assert_eq!(reqs[1].body, "username=123&password=p%26ss%20w");
}

#[test]
fn follows_redirects_and_reports_the_final_url() {
    let srv = fake_server(|s| {
        if s.url == "/inicio" {
            Reply::text(302, "").with("Location", "/pluginfile.php/1/Enunciado%20LP3.pdf")
        } else {
            Reply::bytes(200, b"%PDF", &[("Content-Type", "application/pdf")])
        }
    });
    let r = client().get(&format!("{}/inicio", srv.base), &[]).unwrap();
    assert_eq!(r.status, 200);
    assert!(r.url.ends_with("/pluginfile.php/1/Enunciado%20LP3.pdf"), "url final: {}", r.url);
    assert_eq!(r.body, b"%PDF");
}

#[test]
fn keeps_cookies_between_requests_of_the_same_client() {
    let srv = fake_server(|s| {
        if s.url == "/login" {
            Reply::text(200, "ok").with("Set-Cookie", "MoodleSession=abc123; Path=/")
        } else {
            Reply::text(200, "ok")
        }
    });
    let c = client();
    c.get(&format!("{}/login", srv.base), &[]).unwrap();
    c.get(&format!("{}/my/", srv.base), &[]).unwrap();
    let reqs = srv.requests();
    assert_eq!(reqs[1].header("cookie"), Some("MoodleSession=abc123"));
    // outro cliente tem outro frasco de cookies
    client().get(&format!("{}/my/", srv.base), &[]).unwrap();
    assert!(srv.requests()[2].header("cookie").is_none());
}

#[test]
fn error_status_is_returned_not_raised_and_dead_servers_are_errors() {
    let srv = fake_server(|_| Reply::json(401, json!({"error": "x"})));
    let r = client().get(&format!("{}/x", srv.base), &[]).unwrap();
    assert_eq!(r.status, 401);
    assert!(!r.is_success());
    let base = srv.base.clone();
    drop(srv);
    std::thread::sleep(Duration::from_millis(200));
    assert!(client().get(&format!("{base}/x"), &[]).is_err());
}

#[test]
fn large_bodies_are_read_completely() {
    let payload = vec![b'x'; 3_000_000];
    let p2 = payload.clone();
    let srv = fake_server(move |_| Reply::bytes(200, &p2, &[]));
    let r = client().get(&format!("{}/big", srv.base), &[]).unwrap();
    assert_eq!(r.body.len(), payload.len());
}
