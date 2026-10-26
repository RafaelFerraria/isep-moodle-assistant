import os
import sys
import json
import subprocess
import datetime
import urllib.parse

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
ROOT_DIR_BOOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR_BOOT not in sys.path:
    sys.path.insert(0, ROOT_DIR_BOOT)
from isep_assistant.mail_fetcher import get_emails

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_FILE = os.path.join(BASE_DIR, "config.json")
SESSION_SCRIPT = os.path.join(BASE_DIR, "session_sender.cjs")

def clear():
    os.system("cls" if os.name == "nt" else "clear")

def banner():
    print("=" * 65)
    print("      CONFIGURADOR DO NOTIFICADOR DE EMAILS DO ISEP")
    print("             Envio Automático para o SESSION        ")
    print("=" * 65)
    print()

def generate_session_bot():
    """Generates a new session bot account via session_sender.cjs."""
    try:
        res = subprocess.run(["node", SESSION_SCRIPT, "--generate"], capture_output=True, text=True, check=True)
        data = json.loads(res.stdout.strip())
        return data["mnemonic"], data["sessionId"]
    except Exception as e:
        print(f"[Erro ao gerar Bot Session] {e}")
        return None, None

def test_session_message(to_id: str, mnemonic: str) -> bool:
    print(f"\nA enviar mensagem de teste para {to_id}...")
    test_text = (
        "🤖 Notificador ISEP ligado com sucesso ao Session!\n\n"
        "Este grupo passará a receber diariamente às 18:00 (ou quando o PC ligar) "
        "o resumo dos emails recebidos organizados por ordem de prioridade."
    )
    temp_file = os.path.join(BASE_DIR, "_test_msg.txt")
    with open(temp_file, "w", encoding="utf-8") as f:
        f.write(test_text)
    
    try:
        cmd = ["node", SESSION_SCRIPT, "--to", to_id, "--file", temp_file, "--mnemonic", mnemonic]
        subprocess.run(cmd, capture_output=True, text=True, check=True)
        print("✅ Mensagem de teste enviada com sucesso para o Session!")
        return True
    except subprocess.CalledProcessError as err:
        print("❌ Falha no envio para o Session:")
        print(err.stderr or err.stdout)
        return False
    finally:
        if os.path.exists(temp_file):
            os.remove(temp_file)

def register_scheduled_tasks():
    runner_bat = os.path.join(BASE_DIR, "executar_notificador.bat")
    
    # 1. Daily at 18:00
    task_daily = "ISEP_Email_Session_Daily18h"
    cmd_daily = [
        "schtasks", "/Create", "/F",
        "/TN", task_daily,
        "/TR", f'"{runner_bat}"',
        "/SC", "DAILY",
        "/ST", "18:00"
    ]
    
    # 2. On Logon (after PC boots & user logs in)
    task_logon = "ISEP_Email_Session_OnLogon"
    cmd_logon = [
        "schtasks", "/Create", "/F",
        "/TN", task_logon,
        "/TR", f'"{runner_bat}"',
        "/SC", "ONLOGON"
    ]

    print("\nA configurar o Agendador de Tarefas do Windows...")
    try:
        subprocess.run(cmd_daily, check=True, capture_output=True)
        print("✅ Tarefa diária das 18:00 registada com sucesso!")
    except Exception as e:
        print(f"⚠️ Aviso ao criar tarefa das 18h: {e}")

    try:
        subprocess.run(cmd_logon, check=True, capture_output=True)
        print("✅ Tarefa ao iniciar sessão (após ligar o PC) registada com sucesso!")
    except Exception as e:
        print(f"⚠️ Aviso ao criar tarefa ao iniciar sessão: {e}")

def main():
    clear()
    banner()

    config = {}
    if os.path.exists(CONFIG_FILE):
        try:
            with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                config = json.load(f)
            print("ℹ️ Configuração anterior encontrada.")
        except Exception:
            pass

    # -------------------------------------------------------------
    # 1. CONFIGURAÇÃO DO EMAIL ISEP / OUTLOOK
    # -------------------------------------------------------------
    print("[ETAPA 1/3] Acesso ao Email do ISEP (Outlook)")
    print("-" * 55)
    print("Como desejas aceder aos teus emails do ISEP?")
    print("  [1] Microsoft 365 Login Direto (Autenticação no browser - Mais Seguro)")
    print("  [2] Aplicação Outlook Desktop (Classic Outlook já instalado)")
    print("  [3] IMAP com Palavra-Passe de Aplicação (outlook.office365.com)")
    
    choice = input("\nEscolhe a opção [1-3] (padrão: 1): ").strip() or "1"
    
    if choice == "1":
        config["email_method"] = "graph_api"
        print("\nPara aceder via Microsoft 365, podes usar um Client ID de aplicação Azure.")
        print("(Se já tens uma App Registration no portal Azure do ISEP/IPP ou pessoal, insere o Client ID).")
        client_id = input(f"Client ID da App Azure [{config.get('ms_client_id', '')}]: ").strip()
        if client_id:
            config["ms_client_id"] = client_id
        config["ms_tenant_id"] = "d51de596-45a0-4aa5-a0d8-47d1c958aa32"  # IPP Tenant ID

    elif choice == "2":
        config["email_method"] = "outlook_desktop"
        print("Configurado para ler diretamente da aplicação Outlook do Windows.")

    elif choice == "3":
        config["email_method"] = "imap"
        config["imap_host"] = "outlook.office365.com"
        config["imap_port"] = 993
        email_addr = input(f"Teu email do ISEP (ex: 1XXXXXX@isep.ipp.pt) [{config.get('email_address', '')}]: ").strip()
        if email_addr:
            config["email_address"] = email_addr
        pwd = input("Palavra-passe de Aplicação (App Password do Office 365): ").strip()
        if pwd:
            config["email_password"] = pwd

    print("\nA testar ligação ao email...")
    try:
        since_test = datetime.datetime.now() - datetime.timedelta(days=2)
        test_emails = get_emails(config, since_test)
        print(f"✅ Ligação ao email estabelecida com sucesso! ({len(test_emails)} emails recentes encontrados).")
    except Exception as e:
        print(f"⚠️ Nota de teste ao aceder aos emails: {e}")
        print("Poderás reconfigurar ou concluir o login na primeira execução.\n")

    # -------------------------------------------------------------
    # 2. CONFIGURAÇÃO DO SESSION
    # -------------------------------------------------------------
    print("\n" + "="*55)
    print("[ETAPA 2/3] Configuração do Session Messenger")
    print("-" * 55)
    print("ℹ️ NOTA SOBRE O SESSION:")
    print("  • Os grupos fechados do Session NÃO têm ID visível na aplicação.")
    print("  • Para adicionar o Bot a um grupo, tens de lhe enviar 1 mensagem primeiro")
    print("    (para que o pedido de mensagem seja aceite mutuamente).")
    print()
    print("Como desejas configurar o envio?")
    print("  [1] Assistente de Emparelhamento (Recomendado - Liga ao Grupo ou Chat)")
    print("  [2] Receber no teu Chat Pessoal (Basta colar o teu Session ID 05...)")
    print("  [3] Inserir ID de Grupo / Destinatário manualmente")

    dest_choice = input("\nEscolhe a opção [1-3] (padrão: 1): ").strip() or "1"

    if dest_choice == "1":
        print("\nA iniciar o assistente de emparelhamento do Session...")
        subprocess.run(["node", SESSION_SCRIPT, "--pair", "--config", CONFIG_FILE])
        if os.path.exists(CONFIG_FILE):
            with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                config = json.load(f)

    elif dest_choice == "2":
        if not config.get("session_mnemonic"):
            mnemonic, session_id = generate_session_bot()
            config["session_mnemonic"] = mnemonic
            config["session_bot_id"] = session_id

        print("\nAbre o Session -> Definições (ícone de engrenagem) -> Perfil / Conta.")
        print("Copia o teu Session ID pessoal (começa por '05...').")
        personal_id = input("Cola o teu Session ID pessoal: ").strip()
        if personal_id:
            config["session_group_id"] = personal_id
            print(f"✅ Destino configurado para o teu Session ID: {personal_id}")

    else:
        if not config.get("session_mnemonic"):
            mnemonic, session_id = generate_session_bot()
            config["session_mnemonic"] = mnemonic
            config["session_bot_id"] = session_id
        group_id = input(f"ID do Destino Session [{config.get('session_group_id', '')}]: ").strip()
        if group_id:
            config["session_group_id"] = group_id

    if config.get("session_group_id") and config.get("session_mnemonic"):
        test_session = input("\nDesejas enviar uma mensagem de teste para o Session agora? (s/n): ").strip().lower()
        if test_session in ["s", "sim", "y", "yes"]:
            test_session_message(config["session_group_id"], config["session_mnemonic"])

    # -------------------------------------------------------------
    # 3. CONFIGURAÇÃO DE IA (OPENAI)
    # -------------------------------------------------------------
    print("\n" + "="*55)
    print("[ETAPA 3/3] Inteligência Artificial para Resumos")
    print("-" * 55)
    env_key = os.environ.get("OPENAI_API_KEY")
    if env_key:
        print(f"✅ Chave OPENAI_API_KEY detetada nas variáveis de ambiente do Windows!")
        config["openai_api_key"] = env_key
    else:
        key = input("Chave OpenAI API (opcional, prime Enter para modo heurístico sem IA): ").strip()
        if key:
            config["openai_api_key"] = key

    config["openai_model"] = "gpt-4o-mini"

    # Save config
    with open(CONFIG_FILE, "w", encoding="utf-8") as f:
        json.dump(config, f, indent=2, ensure_ascii=False)
    print(f"\n✅ Configuração guardada com sucesso em: {CONFIG_FILE}")

    # -------------------------------------------------------------
    # 4. AGENDAMENTO NO WINDOWS
    # -------------------------------------------------------------
    print("\n" + "="*55)
    print("Agendamento de Execução Automática")
    print("-" * 55)
    print("Desejas ativar o agendamento no Windows para correr:")
    print("  • Todos os dias às 18:00")
    print("  • Sempre que ligares o computador (ao iniciar sessão)")
    agendar = input("Ativar agendamento automático agora? (S/n): ").strip().lower()
    if agendar in ["", "s", "sim", "y", "yes"]:
        register_scheduled_tasks()

    print("\n" + "="*65)
    print("🎉 CONFIGURAÇÃO CONCLUÍDA COM SUCESSO!")
    print("=" * 65)
    print("Ficheiros criados:")
    print("  • config.json             - Guarda as tuas preferências")
    print("  • executar_notificador.bat - Executa o script manualmente ou via tarefa")
    print("  • main.py                 - Script principal de consulta, resumo e envio")
    print("  • notifier.log            - Registo de atividade e erros")
    print("\nPodes testar a execução a qualquer momento correndo:")
    print("  executar_notificador.bat")
    print("=" * 65 + "\n")

if __name__ == "__main__":
    main()
