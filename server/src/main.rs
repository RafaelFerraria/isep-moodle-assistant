//! isep-assistant-api: servidor local e tarefas agendadas.
//!
//!   isep-assistant-api serve [--port N]     API local (a interface gráfica liga-se aqui)
//!   isep-assistant-api digest [--preview]   resumo diário (chamado pelo Agendador de Tarefas)
//!   --root <pasta>                          raiz de dados (opcional, em qualquer comando)
//!   isep-assistant-api sync                 sincroniza o Moodle agora
//!   isep-assistant-api version
use isep_assistant::api::{security, ApiServer, App};
use isep_assistant::digest::{run_digest, Deps};
use isep_assistant::{config, moodle_sync, service, VERSION};

fn usage() -> i32 {
    eprintln!("Uso: isep-assistant-api <serve [--port N] | digest [--preview] | sync | fetch-thunderbird --to <pasta> | version>");
    2
}

fn serve(args: &[String]) -> i32 {
    let mut port = 0u16;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--port" => match it.next().and_then(|v| v.parse().ok()) {
                Some(p) => port = p,
                None => return usage(),
            },
            _ => return usage(),
        }
    }
    let _ = isep_assistant::views::prune();
    let token = security::new_token();
    let server = match ApiServer::bind(port, App::new(&token)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    // a porta é reservada antes de escrever o ficheiro de estado: a UI nunca lê uma porta errada
    match security::write_state(server.port, &token) {
        Ok(path) => println!("ISEP Assistant API a escutar em http://127.0.0.1:{} (estado em {})", server.port, path.display()),
        Err(e) => {
            eprintln!("não foi possível escrever o ficheiro de estado: {e}");
            return 1;
        }
    }
    server.serve();
    security::clear_state();
    0
}

/// `--root <pasta>` (em qualquer posição) fixa a raiz de dados: é o que as tarefas agendadas passam.
fn take_root(args: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--root" {
            if let Some(dir) = it.next() {
                std::env::set_var("ISEP_ASSISTANT_ROOT", dir);
            }
        } else {
            out.push(a);
        }
    }
    out
}

/// `fetch-thunderbird --to <pasta>`: descarrega, confere e instala o Thunderbird oficial nessa pasta. Usado ao gerar a app,
/// para o Thunderbird já ir dentro dela (o utilizador final não descarrega nada).
fn fetch_thunderbird(args: &[String]) -> i32 {
    let mut it = args.iter();
    let mut to: Option<String> = None;
    while let Some(a) = it.next() {
        match (a.as_str(), it.next()) {
            ("--to", Some(dir)) => to = Some(dir.clone()),
            _ => return usage(),
        }
    }
    let Some(dir) = to else { return usage() };
    std::env::set_var("ISEP_TB_DIR", &dir);
    match isep_assistant::thunderbird::install(&|l| println!("{l}")) {
        Ok(state) => {
            println!("Thunderbird {} instalado em {dir}", state["version"].as_str().unwrap_or("?"));
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn main() {
    let args = take_root(std::env::args().skip(1).collect());
    let code = match args.first().map(String::as_str) {
        Some("serve") => serve(&args[1..]),
        Some("digest") => {
            let preview = args.iter().any(|a| a == "--preview");
            let res = run_digest(!preview, "scheduled", None, None, &Deps::default(), true);
            let _ = isep_assistant::views::prune();
            i32::from(!res.ok)
        }
        Some("sync") => {
            let cfg = config::load_config();
            let dir = isep_assistant::digest::moodle_dir_of(&cfg);
            let Some(base) = service::moodle_base_url(&cfg) else {
                eprintln!("{}", service::NO_MOODLE_URL);
                std::process::exit(1);
            };
            let res = moodle_sync::run_sync("manual", dir.as_deref(), Some(&base), None);
            i32::from(!res.ok && !res.busy)
        }
        Some("fetch-thunderbird") => fetch_thunderbird(&args[1..]),
        Some("version") | Some("--version") => {
            println!("isep-assistant {VERSION}");
            0
        }
        _ => usage(),
    };
    std::process::exit(code);
}
