"""Arranque do servidor local:  python -m isep_assistant.api.server [--port N]

Escuta só em 127.0.0.1. Escreve a porta e o token em %APPDATA%/ISEPAssistant/api.json para a UI se ligar.
Com --port 0 (predefinição) o sistema operativo escolhe uma porta livre.
"""
import argparse
import socket
import sys

from .app import create_app
from .security import clear_state, new_token, write_state


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="API local do ISEP Moodle Assistant")
    parser.add_argument("--port", type=int, default=0, help="porta (0 = escolher uma livre)")
    args = parser.parse_args(argv)

    import uvicorn  # importação tardia: só o servidor precisa dele

    # reservar a porta antes de escrever o ficheiro de estado, para a UI nunca ler uma porta errada
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("127.0.0.1", args.port))
    port = sock.getsockname()[1]
    token = new_token()
    state_path = write_state(port, token)
    print(f"ISEP Assistant API a escutar em http://127.0.0.1:{port} (estado em {state_path})", flush=True)

    config = uvicorn.Config(create_app(token), log_level="warning")
    server = uvicorn.Server(config)
    try:
        server.run(sockets=[sock])
    finally:
        clear_state()
    return 0


if __name__ == "__main__":
    sys.exit(main())
