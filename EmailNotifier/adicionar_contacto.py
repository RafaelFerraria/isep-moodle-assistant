import os
import sys
import json
import subprocess

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_FILE = os.path.join(BASE_DIR, "config.json")
SESSION_SCRIPT = os.path.join(BASE_DIR, "session_sender.cjs")

def main():
    print("=" * 65)
    print("      ADICIONAR DESTINATÁRIO AO NOTIFICADOR ISEP")
    print("=" * 65)
    print()

    if not os.path.exists(CONFIG_FILE):
        print("ERRO: config.json não encontrado.")
        return

    with open(CONFIG_FILE, "r", encoding="utf-8") as f:
        config = json.load(f)

    recipients = config.get("session_recipients", [])
    if isinstance(recipients, str):
        recipients = [recipients]

    print(f"Destinatários atuais ({len(recipients)}):")
    for idx, r in enumerate(recipients, 1):
        print(f"  {idx}. {r}")
    print()

    print("Instruções para o teu colega:")
    print("1. Ele só precisa de ter a app do Session instalada;")
    print("2. Pede-lhe para copiar o Session ID dele (Definições -> Conta / Perfil, 05...);")
    print("3. Ele deve enviar uma mensagem qualquer (ex: 'Ola') para o teu Bot:")
    print(f"   🤖 Bot Session ID: {config.get('session_bot_id')}")
    print("   (Isto garante que o Session dele aceita o bot como contacto!).\n")

    novo_id = input("Cola aqui o Session ID do teu colega (05...): ").strip()
    if not novo_id:
        print("Operação cancelada.")
        return

    if not novo_id.startswith("05") or len(novo_id) != 66:
        print("⚠️ Aviso: O Session ID deve ter 66 caracteres e começar por '05'.")
        confirm = input("Desejas adicionar na mesma? (s/N): ").strip().lower()
        if confirm not in ["s", "sim", "y"]:
            return

    if novo_id not in recipients:
        recipients.append(novo_id)
        config["session_recipients"] = recipients
        with open(CONFIG_FILE, "w", encoding="utf-8") as f:
            json.dump(config, f, indent=2, ensure_ascii=False)
        print(f"\n✅ Contacto adicionado com sucesso à lista de envio!")
    else:
        print("\nℹ️ Este contacto já constava da lista.")

    testar = input("\nDesejas enviar uma mensagem de boas-vindas para o teu colega agora? (S/n): ").strip().lower()
    if testar in ["", "s", "sim", "y", "yes"]:
        msg = (
            "👋 Olá! Foste adicionado à lista de receção do Notificador de Emails do ISEP.\n\n"
            "Passarás a receber aqui diariamente às 18:00 (e ao ligar o PC) o resumo executivo "
            "dos emails da faculdade organizados por ordem de prioridade (prazos Moodle, aulas e avaliações)!"
        )
        temp_file = os.path.join(BASE_DIR, "_temp_welcome.txt")
        with open(temp_file, "w", encoding="utf-8") as f:
            f.write(msg)
        try:
            print(f"A enviar mensagem para {novo_id[:10]}...")
            cmd = ["node", SESSION_SCRIPT, "--to", novo_id, "--file", temp_file, "--mnemonic", config.get("session_mnemonic")]
            subprocess.run(cmd, check=True, capture_output=True)
            print("🎉 Mensagem de boas-vindas entregue com sucesso no Session do teu colega!")
        except Exception as err:
            print(f"Aviso ao enviar mensagem: {err}")
        finally:
            if os.path.exists(temp_file):
                os.remove(temp_file)

if __name__ == "__main__":
    main()
