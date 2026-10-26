"""Proteção da API local: só aceita pedidos de localhost com o token da sessão.

Sem isto, qualquer página web aberta no browser poderia chamar http://127.0.0.1:<porta> (CSRF) e
um ataque de DNS rebinding conseguiria o mesmo com outro nome de host. Por isso:
  - o token é obrigatório (Authorization: Bearer ...), gerado em cada arranque do servidor;
  - o cabeçalho Host tem de ser localhost/127.0.0.1/[::1];
  - não há CORS: browsers não conseguem ler as respostas de outras origens.
"""
import json
import os
import secrets
from typing import Optional

from fastapi import HTTPException, Request
from starlette.responses import JSONResponse

from .. import paths

ALLOWED_HOSTS = {"127.0.0.1", "localhost", "[::1]", "::1"}


def new_token() -> str:
    return secrets.token_urlsafe(32)


def host_allowed(host_header: Optional[str]) -> bool:
    if not host_header:
        return False
    host = host_header.strip().lower()
    if host.startswith("["):  # IPv6 literal: [::1]:8765
        host = host.split("]")[0] + "]"
    else:
        host = host.rsplit(":", 1)[0] if ":" in host else host
    return host in ALLOWED_HOSTS


async def host_guard(request: Request, call_next):
    if not host_allowed(request.headers.get("host")):
        return JSONResponse({"detail": "Host não permitido."}, status_code=403)
    return await call_next(request)


def make_auth_dependency(token: str):
    def require_token(request: Request) -> None:
        header = request.headers.get("authorization", "")
        supplied = header[7:] if header.lower().startswith("bearer ") else ""
        if not supplied or not secrets.compare_digest(supplied, token):
            raise HTTPException(status_code=401, detail="Token em falta ou inválido.",
                                headers={"WWW-Authenticate": "Bearer"})
    return require_token


def write_state(port: int, token: str) -> str:
    """Guarda porta e token para a UI se ligar (ficheiro na pasta do utilizador)."""
    path = paths.api_state_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = str(path) + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump({"port": port, "token": token, "pid": os.getpid()}, f)
    os.replace(tmp, path)
    return str(path)


def clear_state() -> None:
    try:
        os.remove(paths.api_state_file())
    except OSError:
        pass
