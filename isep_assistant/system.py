"""Integração com o Windows: Agendador de Tarefas, Thunderbird, Node.js e rede."""
import json
import os
import shutil
import socket
import subprocess
from typing import Any, Dict, List, Optional, Tuple

from . import paths

TASK_NAMES = ["ISEP_Email_Session_Daily", "ISEP_Email_Session_OnLogon"]
IS_WINDOWS = os.name == "nt"


# ---------------------------------------------------------------- verificações simples

def find_thunderbird_path() -> Optional[str]:
    candidates = [
        os.path.join(os.environ.get("ProgramFiles", "C:\\Program Files"), "Mozilla Thunderbird", "thunderbird.exe"),
        os.path.join(os.environ.get("ProgramFiles(x86)", "C:\\Program Files (x86)"), "Mozilla Thunderbird", "thunderbird.exe"),
        os.path.join(os.environ.get("LOCALAPPDATA", ""), "Programs", "Thunderbird", "thunderbird.exe"),
    ]
    return next((c for c in candidates if c and os.path.exists(c)), None)


def node_path() -> Optional[str]:
    return shutil.which("node")


def internet_ok(host: str = "moodle.isep.ipp.pt", timeout: float = 3.0) -> bool:
    try:
        socket.setdefaulttimeout(timeout)
        socket.gethostbyname(host)
        return True
    except OSError:
        return False


# ---------------------------------------------------------------- Agendador de Tarefas

def parse_tasks_json(text: str) -> List[Dict[str, Any]]:
    """Normaliza a saída JSON do PowerShell (um objeto ou uma lista; datas 'nunca' viram None)."""
    text = (text or "").strip()
    if not text:
        return []
    data = json.loads(text)
    if isinstance(data, dict):
        data = [data]
    out = []
    for t in data:
        def clean(v):
            return None if not v or str(v).startswith(("0001", "1999", "1899")) else v
        out.append({"name": t.get("name"), "state": t.get("state"), "last_run": clean(t.get("last_run")),
                    "next_run": clean(t.get("next_run")), "last_result": t.get("last_result")})
    return out


def _fake_scheduler() -> bool:
    """Com ISEP_FAKE_SCHEDULER definido (testes) o Agendador real nunca é lido nem alterado."""
    return bool(os.environ.get("ISEP_FAKE_SCHEDULER"))


def query_tasks(names: Optional[List[str]] = None) -> List[Dict[str, Any]]:
    """Estado das tarefas agendadas (vazio fora do Windows ou se não existirem)."""
    if _fake_scheduler():
        return [{"name": TASK_NAMES[0], "state": "Ready", "last_run": "2026-10-26T18:00:00",
                 "next_run": "2026-10-27T18:00:00", "last_result": 0}]
    if not IS_WINDOWS:
        return []
    names = names or TASK_NAMES
    quoted = ",".join(f"'{n}'" for n in names)
    ps = (
        f"$r=@(); foreach($n in @({quoted})){{ $t=Get-ScheduledTask -TaskName $n -ErrorAction SilentlyContinue; "
        "if($t){ $i=$t | Get-ScheduledTaskInfo; "
        "$r += [pscustomobject]@{name=$n; state=[string]$t.State; "
        "last_run=$i.LastRunTime.ToString('s'); next_run=$i.NextRunTime.ToString('s'); last_result=[int64]$i.LastTaskResult} } }; "
        "if($r.Count -gt 0){ $r | ConvertTo-Json -Compress }"
    )
    try:
        res = subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-Command", ps],
                             capture_output=True, text=True, encoding="utf-8", timeout=30)
        return parse_tasks_json(res.stdout)
    except (OSError, subprocess.TimeoutExpired, ValueError):
        return []


def register_tasks(scheduled_time: str, runner_bat: Optional[str] = None) -> Tuple[bool, str]:
    """(Re)cria as tarefas diária (hora + retoma da suspensão) e ao iniciar sessão."""
    if _fake_scheduler():
        return True, f"(simulado) tarefas registadas para as {scheduled_time}."
    if not IS_WINDOWS:
        return False, "O Agendador de Tarefas só existe no Windows."
    runner = str(runner_bat or paths.digest_runner_bat())
    ps_cmd = f"""
    $ErrorActionPreference = 'Stop'
    $workDir = Split-Path "{runner}"
    $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\\$env:USERNAME" -LogonType Interactive

    # O PC costuma estar em suspensao a hora agendada (WakeToRun nao o acorda), por isso
    # a tarefa diaria tambem dispara ao retomar da suspensao (Power-Troubleshooter, evento 1).
    $triggerDaily = New-ScheduledTaskTrigger -Daily -At "{scheduled_time}"
    $evClass = Get-CimClass -Namespace Root/Microsoft/Windows/TaskScheduler -ClassName MSFT_TaskEventTrigger
    $triggerResume = New-CimInstance -CimClass $evClass -ClientOnly
    $triggerResume.Enabled = $true
    $triggerResume.Delay = 'PT1M'
    $triggerResume.Subscription = '<QueryList><Query Id="0" Path="System"><Select Path="System">*[System[Provider[@Name=''Microsoft-Windows-Power-Troubleshooter''] and EventID=1]]</Select></Query></QueryList>'
    $actionDaily = New-ScheduledTaskAction -Execute "{runner}" -WorkingDirectory $workDir
    $settingsDaily = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -WakeToRun -RunOnlyIfNetworkAvailable -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 5)
    Register-ScheduledTask -TaskName "{TASK_NAMES[0]}" -Trigger @($triggerDaily, $triggerResume) -Action $actionDaily -Settings $settingsDaily -Principal $principal -Force

    $triggerLogon = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\\$env:USERNAME"
    $triggerLogon.Delay = 'PT1M'
    $actionLogon = New-ScheduledTaskAction -Execute "{runner}" -WorkingDirectory $workDir
    $settingsLogon = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -RunOnlyIfNetworkAvailable -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 5)
    Register-ScheduledTask -TaskName "{TASK_NAMES[1]}" -Trigger $triggerLogon -Action $actionLogon -Settings $settingsLogon -Principal $principal -Force
    """
    try:
        subprocess.run(["powershell", "-NoProfile", "-Command", ps_cmd], check=True, capture_output=True,
                       text=True, timeout=60)
        return True, f"Tarefas registadas (diária às {scheduled_time}, ao retomar da suspensão e ao iniciar sessão)."
    except subprocess.CalledProcessError as err:
        return False, (err.stderr or err.stdout or str(err)).strip()[:300]
    except (OSError, subprocess.TimeoutExpired) as err:
        return False, str(err)
