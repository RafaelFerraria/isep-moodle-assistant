//! Integração com o Windows: Agendador de Tarefas, Thunderbird, Node.js e rede.
use crate::paths;
use crate::util::{run_with_timeout, which};
use serde_json::{json, Value};
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

pub const TASK_NAMES: [&str; 2] = ["ISEP_Email_Session_Daily", "ISEP_Email_Session_OnLogon"];
pub const IS_WINDOWS: bool = cfg!(windows);

// ---------------------------------------------------------------- verificações simples

/// O Thunderbird da app, se estiver instalado, senão o do sistema.
pub fn find_thunderbird_path() -> Option<PathBuf> {
    crate::thunderbird::embedded_exe().or_else(find_system_thunderbird_path)
}

pub fn find_system_thunderbird_path() -> Option<PathBuf> {
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let candidates = [
        PathBuf::from(env("ProgramFiles", "C:\\Program Files")).join("Mozilla Thunderbird").join("thunderbird.exe"),
        PathBuf::from(env("ProgramFiles(x86)", "C:\\Program Files (x86)")).join("Mozilla Thunderbird").join("thunderbird.exe"),
        PathBuf::from(env("LOCALAPPDATA", "")).join("Programs").join("Thunderbird").join("thunderbird.exe"),
    ];
    candidates.into_iter().find(|c| c.exists())
}

/// O «Node» que corre o `session_sender.cjs`.
pub struct NodeRuntime {
    pub exe: PathBuf,
    /// É o executável da própria app (Electron) a fazer de Node: precisa de `ELECTRON_RUN_AS_NODE=1`.
    pub electron: bool,
}

impl NodeRuntime {
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.exe);
        if self.electron {
            cmd.env("ELECTRON_RUN_AS_NODE", "1");
        }
        crate::util::hide_window(&mut cmd);
        cmd
    }
}

/// Por ordem: `ISEP_NODE` (testes); o runtime que vem dentro da app (o Electron, que já traz o Node: o utilizador não instala nada);
/// e, em desenvolvimento, o `node` do PATH.
pub fn node_runtime() -> Option<NodeRuntime> {
    if let Some(v) = std::env::var_os("ISEP_NODE") {
        return Some(NodeRuntime { exe: PathBuf::from(v), electron: false });
    }
    if let Some(res) = paths::embedded_resources() {
        let app = res.parent()?.join("ISEP Assistant.exe");
        if app.exists() {
            return Some(NodeRuntime { exe: app, electron: true });
        }
    }
    which("node").map(|exe| NodeRuntime { exe, electron: false })
}

pub fn node_path() -> Option<PathBuf> {
    node_runtime().map(|n| n.exe)
}

/// Resolve o nome (DNS) do anfitrião; `false` se não houver rede.
pub fn internet_ok(host: &str) -> bool {
    (host, 443u16).to_socket_addrs().map(|mut a| a.next().is_some()).unwrap_or(false)
}

// ---------------------------------------------------------------- Agendador de Tarefas

/// Normaliza a saída JSON do PowerShell (um objeto ou uma lista; datas "nunca" viram null).
pub fn parse_tasks_json(text: &str) -> Vec<Value> {
    let text = text.trim().trim_start_matches('\u{feff}');
    if text.is_empty() {
        return vec![];
    }
    let Ok(data) = serde_json::from_str::<Value>(text) else { return vec![] };
    let items = match data {
        Value::Array(a) => a,
        v @ Value::Object(_) => vec![v],
        _ => vec![],
    };
    let clean = |v: &Value| match v.as_str() {
        Some(s) if !s.is_empty() && !s.starts_with("0001") && !s.starts_with("1999") && !s.starts_with("1899") => v.clone(),
        _ => Value::Null,
    };
    items
        .iter()
        .map(|t| {
            json!({
                "name": t.get("name"), "state": t.get("state"),
                "last_run": clean(&t["last_run"]), "next_run": clean(&t["next_run"]),
                "last_result": t.get("last_result"),
                // programa que a tarefa executa e se ainda existe (uma tarefa a apontar para um ficheiro em falta nunca corre)
                "target": t.get("target").cloned().unwrap_or(Value::Null),
                "target_ok": t.get("target_ok").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

/// Com `ISEP_FAKE_SCHEDULER` definido (testes) o Agendador real nunca é lido nem alterado.
fn fake_scheduler() -> bool {
    std::env::var_os("ISEP_FAKE_SCHEDULER").is_some()
}

/// Horas pedidas a `register_tasks` enquanto o Agendador está simulado (para os testes verificarem).
pub static FAKE_REGISTRATIONS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Estado das tarefas agendadas (vazio fora do Windows ou se não existirem).
pub fn query_tasks() -> Vec<Value> {
    if fake_scheduler() {
        return vec![json!({"name": TASK_NAMES[0], "state": "Ready", "last_run": "2026-10-26T18:00:00", "next_run": "2026-10-27T18:00:00", "last_result": 0, "target": null, "target_ok": true})];
    }
    if !IS_WINDOWS {
        return vec![];
    }
    let quoted = TASK_NAMES.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(",");
    let ps = format!(
        "$r=@(); foreach($n in @({quoted})){{ $t=Get-ScheduledTask -TaskName $n -ErrorAction SilentlyContinue; \
         if($t){{ $i=$t | Get-ScheduledTaskInfo; \
         $x=[Environment]::ExpandEnvironmentVariables([string]$t.Actions[0].Execute).Trim('\"'); \
         $r += [pscustomobject]@{{name=$n; state=[string]$t.State; \
         last_run=$i.LastRunTime.ToString('s'); next_run=$i.NextRunTime.ToString('s'); last_result=[int64]$i.LastTaskResult; \
         target=$x; target_ok=[bool]($x -and (Test-Path -LiteralPath $x))}} }} }}; \
         if($r.Count -gt 0){{ $r | ConvertTo-Json -Compress }}"
    );
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &ps]);
    match run_with_timeout(cmd, None, Duration::from_secs(30)) {
        Ok(out) if !out.timed_out => parse_tasks_json(&out.stdout),
        _ => vec![],
    }
}

/// Script PowerShell que (re)cria as tarefas diária (hora + retoma da suspensão) e ao iniciar sessão.
pub fn registration_script(scheduled_time: &str, exe: &str, args: &str) -> String {
    format!(
        r#"
    $ErrorActionPreference = 'Stop'
    $workDir = Split-Path "{exe}"
    $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive

    # O PC costuma estar em suspensao a hora agendada (WakeToRun nao o acorda), por isso
    # a tarefa diaria tambem dispara ao retomar da suspensao (Power-Troubleshooter, evento 1).
    $triggerDaily = New-ScheduledTaskTrigger -Daily -At "{scheduled_time}"
    $evClass = Get-CimClass -Namespace Root/Microsoft/Windows/TaskScheduler -ClassName MSFT_TaskEventTrigger
    $triggerResume = New-CimInstance -CimClass $evClass -ClientOnly
    $triggerResume.Enabled = $true
    $triggerResume.Delay = 'PT1M'
    $triggerResume.Subscription = '<QueryList><Query Id="0" Path="System"><Select Path="System">*[System[Provider[@Name=''Microsoft-Windows-Power-Troubleshooter''] and EventID=1]]</Select></Query></QueryList>'
    $actionDaily = New-ScheduledTaskAction -Execute "{exe}" -Argument '{args}' -WorkingDirectory $workDir
    $settingsDaily = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -WakeToRun -RunOnlyIfNetworkAvailable -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 5)
    Register-ScheduledTask -TaskName "{daily}" -Trigger @($triggerDaily, $triggerResume) -Action $actionDaily -Settings $settingsDaily -Principal $principal -Force

    $triggerLogon = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
    $triggerLogon.Delay = 'PT1M'
    $actionLogon = New-ScheduledTaskAction -Execute "{exe}" -Argument '{args}' -WorkingDirectory $workDir
    $settingsLogon = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -RunOnlyIfNetworkAvailable -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 5)
    Register-ScheduledTask -TaskName "{logon}" -Trigger $triggerLogon -Action $actionLogon -Settings $settingsLogon -Principal $principal -Force
    "#,
        daily = TASK_NAMES[0],
        logon = TASK_NAMES[1],
    )
}

/// (Re)cria as tarefas agendadas. Devolve `(ok, mensagem)`.
pub fn register_tasks(scheduled_time: &str) -> (bool, String) {
    if fake_scheduler() {
        FAKE_REGISTRATIONS.lock().unwrap().push(scheduled_time.to_string());
        return (true, format!("(simulado) tarefas registadas para as {scheduled_time}."));
    }
    if !IS_WINDOWS {
        return (false, "O Agendador de Tarefas só existe no Windows.".into());
    }
    let (exe, args) = paths::digest_command();
    if !exe.exists() {
        return (false, format!("O programa do resumo não existe: {}", exe.display()));
    }
    let script = registration_script(scheduled_time, &exe.to_string_lossy(), &args);
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", &script]);
    match run_with_timeout(cmd, None, Duration::from_secs(60)) {
        Ok(out) if out.status == Some(0) => (true, format!("Tarefas registadas (diária às {scheduled_time}, ao retomar da suspensão e ao iniciar sessão).")),
        Ok(out) if out.timed_out => (false, "tempo limite ao registar as tarefas".into()),
        Ok(out) => {
            let msg = if out.stderr.trim().is_empty() { out.stdout } else { out.stderr };
            (false, msg.trim().chars().take(300).collect())
        }
        Err(e) => (false, e.to_string()),
    }
}
