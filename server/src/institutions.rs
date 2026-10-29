//! Instituições de ensino superior de Portugal e o endereço do Moodle de cada uma.
//!
//! Só levam endereço as que têm um Moodle central que foi confirmado (a página de login respondeu como Moodle). Para as
//! outras a app pede o endereço ao utilizador e valida-o (`probe`). Não se inventam endereços.
use crate::util::{str_of, Json};
use serde_json::{json, Value};

pub struct Institution {
    pub id: &'static str,
    pub name: &'static str,
    pub moodle: Option<&'static str>,
}

const fn inst(id: &'static str, name: &'static str, moodle: Option<&'static str>) -> Institution {
    Institution { id, name, moodle }
}

pub const OTHER: &str = "outra";

pub const INSTITUTIONS: &[Institution] = &[
    // universidades públicas
    inst("ua", "Universidade de Aveiro", None),
    inst("ubi", "Universidade da Beira Interior", Some("https://moodle.ubi.pt")),
    inst("uc", "Universidade de Coimbra", None),
    inst("uevora", "Universidade de Évora", None),
    inst("ualg", "Universidade do Algarve", None),
    inst("uac", "Universidade dos Açores", None),
    inst("uma", "Universidade da Madeira", Some("https://moodle.uma.pt")),
    inst("ulisboa", "Universidade de Lisboa", Some("https://elearning.ulisboa.pt")),
    inst("unl", "Universidade NOVA de Lisboa", None),
    inst("uminho", "Universidade do Minho", None),
    inst("up", "Universidade do Porto", None),
    inst("utad", "Universidade de Trás-os-Montes e Alto Douro", None),
    inst("iscte", "ISCTE – Instituto Universitário de Lisboa", None),
    // institutos politécnicos
    inst("isep", "ISEP – Instituto Superior de Engenharia do Porto (P.PORTO)", Some("https://moodle.isep.ipp.pt")),
    inst("ipp", "Instituto Politécnico do Porto (outras escolas)", None),
    inst("ipb", "Instituto Politécnico de Bragança", None),
    inst("ipca", "Instituto Politécnico do Cávado e do Ave", None),
    inst("ipc", "Instituto Politécnico de Coimbra", None),
    inst("ipcb", "Instituto Politécnico de Castelo Branco", None),
    inst("ipg", "Instituto Politécnico da Guarda", None),
    inst("ipl", "Instituto Politécnico de Lisboa", None),
    inst("ipleiria", "Instituto Politécnico de Leiria", None),
    inst("ipportalegre", "Instituto Politécnico de Portalegre", None),
    inst("ips", "Instituto Politécnico de Setúbal", None),
    inst("ipsantarem", "Instituto Politécnico de Santarém", None),
    inst("ipt", "Instituto Politécnico de Tomar", None),
    inst("ipv", "Instituto Politécnico de Viseu", None),
    inst("ipvc", "Instituto Politécnico de Viana do Castelo", None),
    inst("ipbeja", "Instituto Politécnico de Beja", None),
    // privadas e cooperativas
    inst("ucp", "Universidade Católica Portuguesa", Some("https://moodle.ucp.pt")),
    inst("ulusofona", "Universidade Lusófona", Some("https://moodle.ulusofona.pt")),
    inst("upt", "Universidade Portucalense", None),
    inst("ual", "Universidade Autónoma de Lisboa", None),
    inst("ulusiada", "Universidade Lusíada", None),
    inst("ueuropeia", "Universidade Europeia", None),
    inst("ufp", "Universidade Fernando Pessoa", None),
    inst("ispa", "ISPA – Instituto Universitário", None),
];

/// Moodle do ISEP: o que a app usava antes de existir o perfil do estudante (instalações antigas continuam a funcionar).
pub const LEGACY_MOODLE: &str = "https://moodle.isep.ipp.pt";

pub fn find(id: &str) -> Option<&'static Institution> {
    INSTITUTIONS.iter().find(|i| i.id == id)
}

pub fn catalog() -> Value {
    json!({"items": INSTITUTIONS.iter().map(|i| json!({"id": i.id, "name": i.name, "has_moodle": i.moodle.is_some()})).collect::<Vec<_>>()})
}

/// Normaliza o que o utilizador escreveu: acrescenta https://, tira espaços, barras finais e o caminho de login.
pub fn normalize_url(raw: &str) -> Option<String> {
    let mut url = raw.trim().to_string();
    if url.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        url = format!("https://{url}");
    }
    for tail in ["/login/index.php", "/login", "/my", "/index.php"] {
        if let Some(stripped) = url.strip_suffix(tail) {
            url = stripped.to_string();
        }
    }
    let url = url.trim_end_matches('/').to_string();
    let host = url.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
    (host.contains('.') && host.len() >= 4).then_some(url)
}

/// Endereço do Moodle a usar: `ISEP_MOODLE_BASE_URL` (testes) > o que o utilizador definiu > o da instituição > o do ISEP
/// (só para instalações anteriores ao perfil). `None` quando a instituição escolhida não tem Moodle central conhecido.
pub fn moodle_url(config: &Json) -> Option<String> {
    if let Ok(v) = std::env::var("ISEP_MOODLE_BASE_URL") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let custom = str_of(config, "moodle_url");
    if !custom.is_empty() {
        return Some(custom.to_string());
    }
    match str_of(config, "student_institution") {
        "" => Some(LEGACY_MOODLE.to_string()),
        id => find(id).and_then(|i| i.moodle).map(str::to_owned),
    }
}

/// De onde vem o endereço (para a interface explicar).
pub fn moodle_url_source(config: &Json) -> &'static str {
    if !str_of(config, "moodle_url").is_empty() {
        "custom"
    } else if str_of(config, "student_institution").is_empty() {
        "default"
    } else if moodle_url(config).is_some() {
        "institution"
    } else {
        "missing"
    }
}

pub fn host_of(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or("").to_string()
}

/// O endereço responde como um Moodle? Devolve o endereço normalizado.
pub fn probe(raw: &str) -> Result<String, String> {
    let url = normalize_url(raw).ok_or("Endereço inválido. Exemplo: https://moodle.escola.pt")?;
    let client = crate::http::HttpClient::new(std::time::Duration::from_secs(15))?;
    let resp = client.get(&format!("{url}/login/index.php"), &[]).map_err(|e| format!("Não foi possível ligar a {url}: {e}"))?;
    let page = resp.text().to_lowercase();
    if resp.status == 200 && page.contains("moodle") {
        Ok(url)
    } else {
        Err(format!("{url} não parece ser um Moodle (resposta {}).", resp.status))
    }
}
