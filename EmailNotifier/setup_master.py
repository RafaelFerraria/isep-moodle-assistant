import os
import sys
import re
import json
import datetime
import subprocess

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
if BASE_DIR not in sys.path:
    sys.path.insert(0, BASE_DIR)

ROOT_DIR = os.path.dirname(BASE_DIR)
CONFIG_FILE = os.path.join(BASE_DIR, "config.json")
RUNNER_BAT = os.path.join(ROOT_DIR, "executar_notificador.bat")
SESSION_SCRIPT = os.path.join(BASE_DIR, "session_sender.cjs")

from summarizer import summarize_emails

def clear_screen():
    os.system("cls" if os.name == "nt" else "clear")

def find_thunderbird_path():
    candidates = [
        os.path.join(os.environ.get("ProgramFiles", "C:\\Program Files"), "Mozilla Thunderbird", "thunderbird.exe"),
        os.path.join(os.environ.get("ProgramFiles(x86)", "C:\\Program Files (x86)"), "Mozilla Thunderbird", "thunderbird.exe"),
        os.path.join(os.environ.get("LOCALAPPDATA", ""), "Programs", "Thunderbird", "thunderbird.exe"),
    ]
    for c in candidates:
        if c and os.path.exists(c):
            return c
    return None

def install_thunderbird_if_needed():
    tb_path = find_thunderbird_path()
    if tb_path:
        print(f"✅ Mozilla Thunderbird detetado em: {tb_path}")
        return tb_path

    print("⚠️ Mozilla Thunderbird não encontrado. A iniciar instalação automática...")
    installer = os.path.join(ROOT_DIR, "Instalador_Thunderbird.exe")
    temp_installer = os.path.join(os.environ.get("TEMP", ""), "thunderbird_setup.exe")

    installer_to_use = None
    if os.path.exists(installer):
        installer_to_use = installer
    elif os.path.exists(temp_installer):
        installer_to_use = temp_installer

    if not installer_to_use:
        print("A descarregar instalador oficial do Thunderbird...")
        try:
            url = "https://download.mozilla.org/?product=thunderbird-latest&os=win64&lang=pt-PT"
            subprocess.run(["curl.exe", "-L", url, "-o", temp_installer], check=True)
            installer_to_use = temp_installer
        except Exception as e:
            print(f"Erro ao descarregar instalador: {e}")
            return None

    # Silent install to user directory
    user_target = os.path.join(os.environ.get("LOCALAPPDATA", ""), "Programs", "Thunderbird")
    print("A instalar Mozilla Thunderbird...")
    try:
        subprocess.run([installer_to_use, "/S"], check=True)
    except Exception:
        pass

    tb_path = find_thunderbird_path()
    if not tb_path:
        # Fallback to user directory
        fallback_exe = os.path.join(user_target, "thunderbird.exe")
        if os.path.exists(fallback_exe):
            tb_path = fallback_exe

    if tb_path:
        print(f"✅ Instalação concluída com sucesso: {tb_path}")
    else:
        print("⚠️ Instalação silenciosa concluída. Abre o Thunderbird pelo atalho criado no Ambiente de Trabalho.")

    return tb_path

def register_windows_tasks(scheduled_time: str):
    print(f"\nA configurar o Agendador de Tarefas do Windows para as {scheduled_time} e ao ligar o PC...")

    # Script PowerShell para criar ambas as tarefas sem problemas de espacos no caminho
    ps_cmd = f"""
    $ErrorActionPreference = 'Stop'
    $workDir = Split-Path "{RUNNER_BAT}"
    $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\\$env:USERNAME" -LogonType Interactive

    # O PC costuma estar em suspensao a hora agendada (WakeToRun nao o acorda), por isso
    # a tarefa diaria tambem dispara ao retomar da suspensao (Power-Troubleshooter, evento 1).
    $triggerDaily = New-ScheduledTaskTrigger -Daily -At "{scheduled_time}"
    $evClass = Get-CimClass -Namespace Root/Microsoft/Windows/TaskScheduler -ClassName MSFT_TaskEventTrigger
    $triggerResume = New-CimInstance -CimClass $evClass -ClientOnly
    $triggerResume.Enabled = $true
    $triggerResume.Delay = 'PT1M'
    $triggerResume.Subscription = '<QueryList><Query Id="0" Path="System"><Select Path="System">*[System[Provider[@Name=''Microsoft-Windows-Power-Troubleshooter''] and EventID=1]]</Select></Query></QueryList>'
    $actionDaily = New-ScheduledTaskAction -Execute "{RUNNER_BAT}" -WorkingDirectory $workDir
    $settingsDaily = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -WakeToRun -RunOnlyIfNetworkAvailable -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 5)
    Register-ScheduledTask -TaskName "ISEP_Email_Session_Daily" -Trigger @($triggerDaily, $triggerResume) -Action $actionDaily -Settings $settingsDaily -Principal $principal -Force

    $triggerLogon = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\\$env:USERNAME"
    $triggerLogon.Delay = 'PT1M'
    $actionLogon = New-ScheduledTaskAction -Execute "{RUNNER_BAT}" -WorkingDirectory $workDir
    $settingsLogon = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -RunOnlyIfNetworkAvailable -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 5)
    Register-ScheduledTask -TaskName "ISEP_Email_Session_OnLogon" -Trigger $triggerLogon -Action $actionLogon -Settings $settingsLogon -Principal $principal -Force
    """
    try:
        subprocess.run(["powershell", "-NoProfile", "-Command", ps_cmd], check=True, capture_output=True, text=True)
        print(f"✅ Tarefa diária das {scheduled_time} (e ao retomar da suspensão) registada com sucesso!")
        print("✅ Tarefa ao iniciar sessão (após ligar o PC) registada com sucesso!")
    except subprocess.CalledProcessError as e:
        print(f"Aviso no registo das tarefas agendadas: {e.stderr or e.stdout or e}")
    except Exception as e:
        print(f"Aviso no registo das tarefas agendadas: {e}")

def send_test_summary(config: dict):
    print("\n" + "="*65)
    print("A ENVIAR RESUMO DE TESTE PARA O TEU SESSION...")
    print("="*65)

    ontem = (datetime.datetime.now() - datetime.timedelta(days=1)).strftime("%d/%m/%Y")
    mock_emails = [
        {
            "id": "1",
            "subject": "DEI - Entrega do Trabalho Prático 1 (Prazo Moodle)",
            "sender": "Prof. Doutor António Martins (DEI)",
            "sender_email": "amm@isep.ipp.pt",
            "received_time": f"{ontem} 14:32",
            "body": "Caros estudantes, relembro que o prazo limite improrrogável para submissão do Trabalho Prático 1 no Moodle termina hoje às 23:59. Certifiquem-se de que o arquivo .zip contém o código-fonte e o relatório em PDF com a identificação do grupo."
        },
        {
            "id": "2",
            "subject": "Arquitetura de Computadores - Alteração da Sala de Aula Prática",
            "sender": "Prof.ª Maria João Silva",
            "sender_email": "mjs@isep.ipp.pt",
            "received_time": f"{ontem} 11:15",
            "body": "Aviso a todos os alunos do turno PL2 que a aula prática de amanhã será lecionada excecionalmente no Laboratório B304 em vez da sala habitual."
        },
        {
            "id": "3",
            "subject": "Estruturas de Informação - Guião do Laboratório 4 e Testes Unitários",
            "sender": "Coordenação de Estruturas de Informação",
            "sender_email": "ei-coord@isep.ipp.pt",
            "received_time": f"{ontem} 16:45",
            "body": "Já se encontra publicado na página da unidade curricular no Moodle o Guião 4 dedicado a Árvores Binárias de Pesquisa, juntamente com a suite de testes unitários para preparação."
        },
        {
            "id": "4",
            "subject": "Gabinete de Relações Externas - Candidaturas Erasmus+ 2026/2027",
            "sender": "Gabinete de Relações Externas (GRE)",
            "sender_email": "gre@isep.ipp.pt",
            "received_time": f"{ontem} 09:20",
            "body": "Estão abertas as candidaturas para o programa de mobilidade internacional Erasmus+ para o próximo ano letivo. Sessão de esclarecimento online na próxima quinta-feira às 14h."
        }
    ]

    resumo = summarize_emails(mock_emails, config)
    recipients = config.get("session_recipients") or [config.get("session_group_id")]
    mnemonic = config.get("session_mnemonic")

    temp_msg = os.path.join(BASE_DIR, "_test_summary_msg.txt")
    with open(temp_msg, "w", encoding="utf-8") as f:
        f.write(resumo)

    for target in recipients:
        if not target:
            continue
        try:
            cmd = ["node", SESSION_SCRIPT, "--to", target, "--file", temp_msg, "--mnemonic", mnemonic]
            subprocess.run(cmd, check=True, capture_output=True)
            print(f"🎉 Resumo de teste entregue com sucesso no Session ({target[:10]}...)!")
        except Exception as err:
            print(f"Aviso no envio de teste: {err}")

    if os.path.exists(temp_msg):
        try:
            os.remove(temp_msg)
        except Exception:
            pass

from ai_summarizer import describe


def main():
    clear_screen()
    print("=" * 70)
    print("   NOTIFICADOR INTELIGENTE DE EMAILS DO ISEP PARA SESSION MESSENGER   ")
    print("=" * 70)
    print()
    print("📖 O QUE ESTE SCRIPT FAZ:")
    print("----------------------------------------------------------------------")
    print("Este sistema automatiza a leitura e priorização dos teus emails do ISEP:")
    print("  1. Acede à tua caixa de correio institucional através do Mozilla Thunderbird.")
    print("  2. Analisa todos os emails recebidos diariamente.")
    print("  3. Classifica e resume os emails por ordem estrita de prioridade:")
    print("     🔴 PRIORIDADE ALTA  : Prazos de entrega Moodle, testes, exames e salas.")
    print("     🟡 PRIORIDADE MÉDIA : Guiões de laboratórios e novos materiais de aulas.")
    print("     🟢 PRIORIDADE BAIXA : Avisos institucionais e oportunidades gerais.")
    print("  4. Envia o resumo executivo diretamente para a tua aplicação do Session.")
    print("  5. Executa automaticamente todos os dias na hora que escolheres e sempre")
    print("     que ligares o computador.")
    print()
    print("🛣️ ROTEIRO DOS PROCEDIMENTOS QUE VAMOS SEGUIR (TUDO AUTOMATIZADO):")
    print("----------------------------------------------------------------------")
    print("  [Passo 1] Pedir os teus dados logo no início (Email, Hora, Session ID, modo de resumo).")
    print("  [Passo 2] Verificar e instalar automaticamente o Mozilla Thunderbird.")
    print("  [Passo 3] Abrir o Thunderbird para ligares a tua conta (login único).")
    print("  [Passo 4] Criar as tarefas automáticas no Agendador do Windows.")
    print("  [Passo 5] Enviar um resumo de teste imediato para o teu Session.")
    print("=" * 70)
    print()

    # Load existing config
    config = {}
    current_user = os.environ.get("USERNAME", "")
    if os.path.exists(CONFIG_FILE):
        try:
            with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                config = json.load(f)
        except Exception:
            pass

    configured_user = config.get("windows_user")
    is_new_user = (configured_user is not None and configured_user != current_user)

    # Se for outro utilizador a configurar nesta maquina, gera um novo bot limpo
    if is_new_user or not config.get("session_mnemonic"):
        try:
            res = subprocess.run(["node", SESSION_SCRIPT, "--generate"], check=True, capture_output=True, text=True)
            bot_data = json.loads(res.stdout.strip())
            config["session_mnemonic"] = bot_data["mnemonic"]
            config["session_bot_id"] = bot_data["sessionId"]
        except Exception:
            pass

    # -------------------------------------------------------------
    # PASSO 1: RECOLHA DE DADOS NO INÍCIO
    # -------------------------------------------------------------
    print("[PASSO 1/5] RECOLHA DE DADOS INICIAIS")
    print("-" * 55)

    # 1. Email
    cur_email = config.get("email_address") if not is_new_user else ""
    prompt_email = f" [{cur_email}]" if cur_email else ""
    while True:
        email_in = input(f"1. Teu email institucional do ISEP{prompt_email}: ").strip()
        final_email = email_in if email_in else cur_email
        if final_email and "@" in final_email:
            config["email_address"] = final_email
            break
        print("⚠️ Formato de email inválido! Por favor insere o teu email do ISEP (ex: numero@isep.ipp.pt).")

    # 2. Hora de Execução
    cur_time = config.get("scheduled_time") or "18:00"
    while True:
        time_in = input(f"2. A que horas deve correr o script todos os dias? (HH:MM) [{cur_time}]: ").strip()
        time_val = time_in if time_in else cur_time
        if re.match(r"^([01]?[0-9]|2[0-3]):[0-5][0-9]$", time_val):
            # Normalizar para formato HH:MM (com 2 dígitos na hora)
            parts = time_val.split(":")
            scheduled_time = f"{int(parts[0]):02d}:{int(parts[1]):02d}"
            config["scheduled_time"] = scheduled_time
            break
        else:
            print("⚠️ Formato de hora inválido! Por favor insere no formato HH:MM (ex: 18:00, 08:30, 21:00).")

    # 3. Session ID
    cur_session = config.get("session_group_id") if not is_new_user else ""
    prompt_session = f" [{cur_session[:12]}...]" if cur_session else ""
    while True:
        session_in = input(f"3. Teu Session ID de destino (onde vais receber os resumos){prompt_session}: ").strip()
        final_session = session_in if session_in else cur_session
        if final_session and len(final_session) == 66:
            config["session_group_id"] = final_session
            config["session_recipients"] = [final_session]
            break
        print("⚠️ Session ID inválido! O teu Session ID deve conter 66 caracteres e começar por '05'.")

    # 4. Modo de resumo (regras locais ou IA) — pode ser alterado depois com configurar_resumo.bat
    from configurar_resumo import configure_summary
    configure_summary(config)

    config["email_method"] = "thunderbird"
    # Integração com o MoodleSync (pasta irmã): o resumo diário inclui o que foi descarregado
    moodle_sync_dir = os.path.abspath(os.path.join(BASE_DIR, "..", "MoodleSync"))
    if os.path.exists(os.path.join(moodle_sync_dir, "moodle_sync.py")):
        config.setdefault("moodle_sync_dir", moodle_sync_dir)
    config["windows_user"] = current_user

    # Se for novo utilizador, reinicia o last_run para as ultimas 24h
    if is_new_user:
        last_run_path = os.path.join(BASE_DIR, "last_run.json")
        try:
            with open(last_run_path, "w", encoding="utf-8") as f:
                json.dump({"last_run": (datetime.datetime.now() - datetime.timedelta(hours=24)).isoformat()}, f, indent=2)
        except Exception:
            pass

    # Save initial config
    with open(CONFIG_FILE, "w", encoding="utf-8") as f:
        json.dump(config, f, indent=2, ensure_ascii=False)
    print("\n✅ Dados iniciais recolhidos e guardados com sucesso!")

    # -------------------------------------------------------------
    # PASSO 2: INSTALAÇÃO DO THUNDERBIRD
    # -------------------------------------------------------------
    print("\n" + "="*55)
    print("[PASSO 2/5] VERIFICAÇÃO / INSTALAÇÃO DO THUNDERBIRD")
    print("-" * 55)
    tb_exe = install_thunderbird_if_needed()

    # -------------------------------------------------------------
    # PASSO 3: LOGIN NO THUNDERBIRD
    # -------------------------------------------------------------
    print("\n" + "="*55)
    print("[PASSO 3/5] LIGAÇÃO DA CONTA NO THUNDERBIRD")
    print("-" * 55)
    print("O Thunderbird é o cliente oficial que tem autorização da Microsoft para")
    print("sincronizar os emails do ISEP sem bloqueios de administrador.")
    print()
    print("👉 PROCEDIMENTO NO THUNDERBIRD (Leva 30 segundos):")
    print("  1. Vai abrir a janela do Thunderbird;")
    print(f"  2. Insere o teu nome e o teu email: {config['email_address']}")
    print("  3. Clica em 'Continuar' (o Thunderbird deteta IMAP/OAuth2 automaticamente);")
    print("  4. Clica em 'Concluir' e faz login na janela oficial da Microsoft;")
    print("  5. Deixa o Thunderbird sincronizar os emails da tua Caixa de Entrada.")
    print()

    input("Pressiona [ENTER] para abrir o Thunderbird agora...")
    if tb_exe and os.path.exists(tb_exe):
        try:
            subprocess.Popen([tb_exe])
            print("🚀 Thunderbird iniciado!")
        except Exception as e:
            print(f"Aviso ao abrir Thunderbird: {e}")
    else:
        print("Abre o Thunderbird através do atalho no teu Ambiente de Trabalho.")

    print("\nA aguardar que configures o email no Thunderbird...")
    input("👉 Quando tiveres concluído a ligação no Thunderbird, pressiona [ENTER] para continuar...")

    # -------------------------------------------------------------
    # PASSO 4: AGENDAMENTO AUTOMÁTICO NO WINDOWS
    # -------------------------------------------------------------
    print("\n" + "="*55)
    print("[PASSO 4/5] AGENDAMENTO NO WINDOWS")
    print("-" * 55)
    register_windows_tasks(config["scheduled_time"])

    # -------------------------------------------------------------
    # PASSO 5: TESTE DE ENVIO PARA O SESSION
    # -------------------------------------------------------------
    print("\n" + "="*55)
    print("[PASSO 5/5] ENVIO DO RESUMO DE TESTE")
    print("-" * 55)
    send_test_summary(config)

    print("\n" + "=" * 70)
    print("🎉 TUDO CONFIGURADO E OPERACIONAL!")
    print("=" * 70)
    print(f"• Conta ISEP          : {config['email_address']}")
    print(f"• Hora de Envio Diário : Todos os dias às {config['scheduled_time']}")
    print(f"• Arranque do PC      : Envia também sempre que ligares o computador")
    print(f"• Destino no Session  : {config['session_group_id']}")
    print(f"• Modo de resumo      : {describe(config)}  (mudar: configurar_resumo.bat)")
    print("=" * 70)
    print("\nPodes fechar esta janela. O sistema tratará de tudo automaticamente!")
    print()

if __name__ == "__main__":
    main()
