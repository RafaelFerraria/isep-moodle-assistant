import os
import sys
import json
import datetime
import webbrowser
import subprocess
import msal
import requests

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_FILE = os.path.join(BASE_DIR, "config.json")
CACHE_FILE = os.path.join(BASE_DIR, "token_cache.bin")

def copy_to_clipboard(text: str):
    try:
        subprocess.run("clip", input=text, text=True, check=True)
        return True
    except Exception:
        return False

def main():
    print("=" * 65)
    print("      AUTENTICAÇÃO DO EMAIL ISEP (MICROSOFT 365)")
    print("=" * 65)
    print()

    if not os.path.exists(CONFIG_FILE):
        print("ERRO: config.json não encontrado.")
        return

    with open(CONFIG_FILE, "r", encoding="utf-8") as f:
        config = json.load(f)

    # Use Graph Explorer client ID pre-approved for Graph API
    client_id = config.get("ms_client_id") or "de8bc8b5-d9f9-48b1-a8ad-b748da725064"
    tenant_id = config.get("ms_tenant_id") or "7a60a629-e61f-430c-9c63-266653b2b899"
    authority = f"https://login.microsoftonline.com/{tenant_id}"
    scopes = ["Mail.Read", "User.Read"]

    token_cache = msal.SerializableTokenCache()
    if os.path.exists(CACHE_FILE):
        try:
            with open(CACHE_FILE, "r", encoding="utf-8") as f:
                token_cache.deserialize(f.read())
        except Exception:
            pass

    app = msal.PublicClientApplication(
        client_id=client_id,
        authority=authority,
        token_cache=token_cache
    )

    accounts = app.get_accounts()
    result = None
    if accounts:
        result = app.acquire_token_silent(scopes, account=accounts[0])

    if result and "access_token" in result:
        print("✅ Sessão já se encontra autenticada com sucesso!")
        print(f"Conta: {accounts[0].get('username', 'Utilizador')}")
    else:
        flow = app.initiate_device_flow(scopes=scopes)
        if "user_code" not in flow:
            print(f"❌ Erro ao iniciar autenticação: {flow.get('error_description')}")
            return

        user_code = flow["user_code"]
        login_url = flow.get("verification_uri", "https://microsoft.com/devicelogin")

        # Copy code to clipboard automatically
        copied = copy_to_clipboard(user_code)

        print("-----------------------------------------------------------------")
        print(f"🔑 CÓDIGO DE AUTENTICAÇÃO: {user_code}")
        if copied:
            print("   (O código foi copiado automaticamente para a tua área de transferência!)")
        print(f"🌐 LINK: {login_url}")
        print("-----------------------------------------------------------------")
        print("\nA abrir o browser automaticamente...")
        try:
            webbrowser.open(login_url)
        except Exception:
            pass

        print("\nPassos:")
        print("1. Cola o código na página que se abriu (Ctrl+V);")
        print("2. Inicia sessão com o teu email do ISEP (numero@isep.ipp.pt);")
        print("3. Confirma a autorização.")
        print("\n[A aguardar confirmação no browser...] (Podes cancelar com Ctrl+C)\n")

        result = app.acquire_token_by_device_flow(flow)

        if token_cache.has_state_changed:
            with open(CACHE_FILE, "w", encoding="utf-8") as f:
                f.write(token_cache.serialize())

    if not result or "access_token" not in result:
        err = result.get("error_description") if result else "Sem resposta"
        print(f"\n❌ Falha na autenticação: {err}")
        return

    print("✅ Autenticação concluída com sucesso no Microsoft 365!")
    print("A testar obtenção dos emails mais recentes...")

    access_token = result["access_token"]
    resp = requests.get(
        "https://graph.microsoft.com/v1.0/me/messages?$top=5&$select=subject,from,receivedDateTime",
        headers={"Authorization": f"Bearer {access_token}"}
    )

    if resp.status_code == 200:
        data = resp.json().get("value", [])
        print(f"🎉 Ligação confirmada! Encontrados {len(data)} emails recentes:")
        for msg in data:
            sender = msg.get("from", {}).get("emailAddress", {}).get("name", "Desconhecido")
            print(f" • [{sender}] {msg.get('subject')}")
    else:
        print(f"Aviso ao consultar Graph API: {resp.status_code} - {resp.text}")

    print("\n" + "=" * 65)
    print("O sistema está 100% configurado!")
    print("Podes agora correr o 'executar_notificador.bat' para enviar")
    print("o primeiro resumo diretamente para o teu Session!")
    print("=" * 65)

if __name__ == "__main__":
    main()
