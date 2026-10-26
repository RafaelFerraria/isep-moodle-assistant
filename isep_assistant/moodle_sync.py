"""
Moodle ISEP - Sincronizador Automático de Materiais
Desenvolvido para organizar e descarregar automaticamente todas as disciplinas do Moodle do ISEP.
"""

import os
import sys
import re
import json
import time
import socket
import mimetypes
import hashlib
import ctypes
import ctypes.wintypes
import urllib.parse
from dataclasses import dataclass, field, asdict
from datetime import datetime, date
from typing import Callable, List, Optional
import requests
from bs4 import BeautifulSoup

from . import history, paths

# --- CONSTANTES ---
BASE_URL = "https://moodle.isep.ipp.pt"
LOGIN_URL = f"{BASE_URL}/login/index.php"
AJAX_URL = f"{BASE_URL}/lib/ajax/service.php"
DEFAULT_DOWNLOAD_DIR = str(paths.default_download_dir())
# new_files.jsonl: uma linha JSON por ficheiro novo/atualizado; lida pelo resumo diário

# Quem quiser acompanhar o progresso (a API) regista aqui uma função que recebe cada linha de log
_log_sink: Optional[Callable[[str], None]] = None

# --- SEGURANÇA DPAPI (Encriptação nativa do Windows) ---
class DATA_BLOB(ctypes.Structure):
    _fields_ = [('cbData', ctypes.wintypes.DWORD), ('pbData', ctypes.POINTER(ctypes.c_byte))]

def encrypt_dpapi(text: str) -> str:
    """Encripta texto com a chave da conta de utilizador do Windows atual."""
    data = text.encode('utf-8')
    in_blob = DATA_BLOB(len(data), ctypes.cast(ctypes.create_string_buffer(data, len(data)), ctypes.POINTER(ctypes.c_byte)))
    out_blob = DATA_BLOB()
    if ctypes.windll.crypt32.CryptProtectData(ctypes.byref(in_blob), None, None, None, None, 0, ctypes.byref(out_blob)):
        res = ctypes.string_at(out_blob.pbData, out_blob.cbData)
        ctypes.windll.kernel32.LocalFree(out_blob.pbData)
        return res.hex()
    raise RuntimeError("Falha ao encriptar credenciais com Windows DPAPI.")

def decrypt_dpapi(hex_data: str) -> str:
    """Decifra texto usando a chave da conta de utilizador do Windows atual."""
    data = bytes.fromhex(hex_data)
    in_blob = DATA_BLOB(len(data), ctypes.cast(ctypes.create_string_buffer(data, len(data)), ctypes.POINTER(ctypes.c_byte)))
    out_blob = DATA_BLOB()
    if ctypes.windll.crypt32.CryptUnprotectData(ctypes.byref(in_blob), None, None, None, None, 0, ctypes.byref(out_blob)):
        res = ctypes.string_at(out_blob.pbData, out_blob.cbData)
        ctypes.windll.kernel32.LocalFree(out_blob.pbData)
        return res.decode('utf-8')
    raise RuntimeError("Falha ao decifrar credenciais com Windows DPAPI.")

# --- UTILITÁRIOS ---
def log(message: str, to_console: bool = True):
    timestamp = datetime.now().strftime("[%Y-%m-%d %H:%M:%S]")
    line = f"{timestamp} {message}"
    if to_console:
        try:
            print(line)
        except Exception:
            pass
    try:
        with open(paths.sync_log(), "a", encoding="utf-8") as f:
            f.write(line + "\n")
    except Exception:
        pass
    if _log_sink:
        try:
            _log_sink(message)
        except Exception:
            pass

def sanitize_filename(name: str) -> str:
    """Limpa caracteres inválidos no Windows e corta espaços."""
    name = re.sub(r'[<>:"/\\|?*]', '_', name)
    name = re.sub(r'\s+', ' ', name).strip()
    return name[:120] if len(name) > 120 else (name or "sem_nome")

def wait_for_internet(max_wait_seconds: int = 300) -> bool:
    """Espera pela ligação (DNS incluído) - útil logo após ligar o PC / acordar da suspensão."""
    start = time.time()
    while time.time() - start < max_wait_seconds:
        try:
            socket.gethostbyname("moodle.isep.ipp.pt")
            return True
        except OSError:
            time.sleep(10)
    return False

def load_config() -> dict:
    cfg = paths.sync_config()
    if cfg.exists():
        try:
            with open(cfg, "r", encoding="utf-8-sig") as f:
                return json.load(f)
        except Exception as e:
            log(f"Aviso ao carregar config: {e}")
    return {}

def save_config(config: dict):
    cfg = paths.sync_config()
    cfg.parent.mkdir(parents=True, exist_ok=True)
    tmp = str(cfg) + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(config, f, indent=4, ensure_ascii=False)
    os.replace(tmp, cfg)

def clean_username(user: str) -> str:
    user = user.strip()
    if "@isep.ipp.pt" in user.lower():
        user = user.split("@")[0]
    return user

# --- CLASSE DE SINCRONIZAÇÃO DO MOODLE ---
class MoodleISEPSync:
    def __init__(self, username: str, password: str, download_dir: str, sync_dir: Optional[str] = None):
        self.sync_dir = sync_dir  # pasta com new_files.jsonl e sync.lock (predefinição: MoodleSync/)
        self.busy = False         # True se outra sincronização já estava a decorrer
        self.username = clean_username(username)
        self.password = password
        self.download_dir = download_dir
        self.session = requests.Session()
        self.session.headers.update({
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "Accept-Language": "pt-PT,pt;q=0.9,en;q=0.8"
        })
        self.sesskey = None
        self.errors = 0
        self.new_files = []
        self._ctx = {"course": "", "section": ""}

    def login(self) -> bool:
        log("A autenticar no Moodle do ISEP...")
        try:
            # 1. Carrega página de login para obter cookies iniciais
            r_init = self.session.get(LOGIN_URL, timeout=20)
            
            # 2. Envia credenciais
            login_data = {
                "username": self.username,
                "password": self.password
            }
            r_post = self.session.post(LOGIN_URL, data=login_data, timeout=25, allow_redirects=True)
            
            # 3. Verifica sucesso
            if "errorcode=3" in r_post.url or "divErro" in r_post.text or "Dados de acesso errados" in r_post.text:
                log("Erro: Credenciais de acesso erradas no Moodle ISEP!")
                return False

            if "MoodleSession" not in self.session.cookies:
                log("Erro: Sessão do Moodle não foi estabelecida.")
                return False

            # Procura sesskey no HTML
            match = re.search(r'["\']sesskey["\']\s*:\s*["\']([a-zA-Z0-9]+)["\']', r_post.text)
            if not match:
                match = re.search(r'name=["\']sesskey["\']\s+value=["\']([a-zA-Z0-9]+)["\']', r_post.text)
            if match:
                self.sesskey = match.group(1)

            log("Autenticação com sucesso!")
            return True
        except Exception as e:
            log(f"Erro durante a autenticação: {e}")
            return False

    def get_enrolled_courses(self) -> list:
        """Descobre todas as disciplinas em que o utilizador está inscrito."""
        log("A pesquisar disciplinas inscritas...")
        courses = {}

        # Método A: Via API AJAX Moodle (timeline / enrolled courses)
        if self.sesskey:
            try:
                params = {"sesskey": self.sesskey, "info": "core_course_get_enrolled_courses_by_timeline_classification"}
                payload = [{
                    "index": 0,
                    "methodname": "core_course_get_enrolled_courses_by_timeline_classification",
                    "args": {"offset": 0, "limit": 0, "classification": "all", "sort": "fullname"}
                }]
                r_ajax = self.session.post(AJAX_URL, params=params, json=payload, timeout=20)
                if r_ajax.status_code == 200:
                    data = r_ajax.json()
                    if isinstance(data, list) and len(data) > 0 and "data" in data[0]:
                        course_list = data[0]["data"].get("courses", [])
                        for c in course_list:
                            cid = str(c.get("id"))
                            cname = c.get("fullname") or c.get("shortname") or f"Disciplina_{cid}"
                            if cid != "1":
                                courses[cid] = cname
            except Exception as e:
                log(f"Aviso ao consultar API AJAX de cursos: {e}")

        # Método B: Scraping da página /my/ e homepage
        for page_url in [f"{BASE_URL}/my/", f"{BASE_URL}/"]:
            try:
                r_page = self.session.get(page_url, timeout=20)
                if r_page.status_code == 200:
                    soup = BeautifulSoup(r_page.text, "html.parser")
                    
                    # Se ainda não tínhamos sesskey, tenta encontrar agora
                    if not self.sesskey:
                        match = re.search(r'["\']sesskey["\']\s*:\s*["\']([a-zA-Z0-9]+)["\']', r_page.text)
                        if match:
                            self.sesskey = match.group(1)

                    # Links de disciplinas
                    for a in soup.find_all("a", href=True):
                        href = a["href"]
                        match = re.search(r'/course/view\.php\?id=(\d+)', href)
                        if match:
                            cid = match.group(1)
                            if cid != "1":
                                text = a.get_text(strip=True)
                                if text and len(text) > 2 and cid not in courses:
                                    courses[cid] = text
            except Exception as e:
                log(f"Aviso ao analisar {page_url}: {e}")

        course_list = [{"id": cid, "name": name} for cid, name in courses.items()]
        log(f"Foram encontradas {len(course_list)} disciplinas.")
        return course_list

    def _record(self, kind: str, path: str, size: int):
        entry = {
            "ts": datetime.now().isoformat(timespec="seconds"),
            "kind": kind,  # "novo" | "atualizado"
            "course": self._ctx["course"],
            "section": self._ctx["section"],
            "name": os.path.basename(path),
            "path": os.path.relpath(path, self.download_dir),
            "size": size,
        }
        self.new_files.append(entry)
        try:
            with open(paths.new_files_log(self.sync_dir), "a", encoding="utf-8") as f:
                f.write(json.dumps(entry, ensure_ascii=False) + "\n")
        except Exception:
            pass

    @staticmethod
    def _real_name(r, fallback: str) -> str:
        """Nome real: Content-Disposition > URL final > texto do link (+ extensão pelo Content-Type)."""
        content_disp = r.headers.get("Content-Disposition", "")
        name = None
        m = re.search(r"filename\*=(?:UTF-8|utf-8)''([^;]+)", content_disp)
        if m:
            name = urllib.parse.unquote(m.group(1))
        else:
            m = re.search(r'filename=["\']?([^";\r\n]+)', content_disp)
            if m:
                name = m.group(1)
        if not name and "pluginfile.php" in r.url:  # só aí o último segmento do URL é o nome do ficheiro
            url_name = urllib.parse.unquote(os.path.basename(urllib.parse.urlparse(r.url).path))
            if os.path.splitext(url_name)[1]:
                name = url_name
        if not name:
            name = fallback
        if not os.path.splitext(name)[1]:
            ctype = r.headers.get("Content-Type", "").split(";")[0].strip()
            ext = mimetypes.guess_extension(ctype) if ctype and ctype != "text/html" else None
            if ext:
                name += ext
        return sanitize_filename(name)

    def download_file(self, url: str, dest_path: str) -> bool:
        """Descarrega de forma incremental; só regista como novo se for novo ou tiver mudado."""
        try:
            r = self.session.get(url, stream=True, timeout=30)
            if r.status_code != 200:
                self.errors += 1
                log(f"  [!] HTTP {r.status_code} ao descarregar {url}")
                return False

            legacy_path = dest_path  # nome antigo (sem extensão) usado em versões anteriores
            dest_path = os.path.join(os.path.dirname(dest_path), self._real_name(r, os.path.basename(dest_path)))

            expected_size = r.headers.get("Content-Length")
            existed = os.path.exists(dest_path)
            if existed and expected_size and os.path.getsize(dest_path) == int(expected_size):
                r.close()
                return True

            os.makedirs(os.path.dirname(dest_path), exist_ok=True)
            tmp_path = dest_path + ".tmp"
            with open(tmp_path, "wb") as f:
                for chunk in r.iter_content(chunk_size=16384):
                    if chunk:
                        f.write(chunk)

            def digest(path):
                with open(path, "rb") as fh:
                    return hashlib.sha256(fh.read()).hexdigest()

            if existed and digest(dest_path) == digest(tmp_path):
                os.remove(tmp_path)  # igual ao que já existe (o servidor não enviou Content-Length)
                return True

            if existed:
                os.remove(dest_path)
            os.rename(tmp_path, dest_path)

            # Migração: apaga o ficheiro antigo sem extensão (ex.: "Enunciado LP3 V1.2Ficheiro")
            if legacy_path != dest_path and os.path.isfile(legacy_path) and not os.path.splitext(legacy_path)[1]:
                try:
                    os.remove(legacy_path)
                except OSError:
                    pass

            self._record("atualizado" if existed else "novo", dest_path, os.path.getsize(dest_path))
            log(f"  [+] {'Atualizado' if existed else 'Novo ficheiro'}: {os.path.basename(dest_path)}")
            return True
        except Exception as e:
            self.errors += 1
            log(f"  [!] Erro ao descarregar {url}: {e}")
            return False

    def save_page(self, url: str, title: str, dest_dir: str):
        """Guarda o texto de uma atividade 'Página' (mod/page) e regista se for nova ou tiver mudado."""
        try:
            r = self.session.get(url, timeout=20)
            if r.status_code != 200:
                return
            soup = BeautifulSoup(r.text, "html.parser")
            main = soup.find(attrs={"role": "main"})
            text = main.get_text("\n", strip=True) if main else ""
            if not text:
                return
            path = os.path.join(dest_dir, sanitize_filename(title) + ".txt")
            existed = os.path.exists(path)
            if existed:
                with open(path, "r", encoding="utf-8") as f:
                    if f.read() == text:
                        return
            os.makedirs(dest_dir, exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
            self._record("atualizado" if existed else "novo", path, len(text.encode("utf-8")))
            log(f"  [+] Página {'atualizada' if existed else 'nova'}: {title}")
        except Exception as e:
            self.errors += 1
            log(f"  [!] Erro ao guardar página {title}: {e}")

    def sync_course(self, course_id: str, course_name: str):
        """Sincroniza todos os tópicos e materiais de uma disciplina."""
        clean_cname = sanitize_filename(course_name)
        course_dir = os.path.join(self.download_dir, clean_cname)
        os.makedirs(course_dir, exist_ok=True)
        log(f"\n--- Sincronizando: {clean_cname} (ID: {course_id}) ---")

        course_url = f"{BASE_URL}/course/view.php?id={course_id}"
        r = self.session.get(course_url, timeout=30)
        if r.status_code != 200:
            log(f"Não foi possível aceder à disciplina {course_id}")
            return

        soup = BeautifulSoup(r.text, "html.parser")
        
        # Encontra secções da disciplina
        sections = soup.find_all(["li", "div"], class_=re.compile(r'section\s+main|course-section'))
        if not sections:
            # Fallback para estrutura geral
            sections = [soup]

        for s_idx, sec in enumerate(sections, start=1):
            # Nome da secção
            sec_name_el = sec.find(["h3", "h4", "span"], class_=re.compile(r'sectionname|section-title'))
            sec_name = sec_name_el.get_text(strip=True) if sec_name_el else f"Secção {s_idx:02d}"
            sec_dir = os.path.join(course_dir, f"{s_idx:02d} - {sanitize_filename(sec_name)}")
            self._ctx = {"course": course_name, "section": sec_name}

            # 1. Recursos individuais (PDFs, docs, ficheiros)
            resources = sec.find_all("a", href=re.compile(r'/mod/resource/view\.php\?id=\d+'))
            for res_a in resources:
                res_url = res_a["href"]
                res_label = res_a.get_text(strip=True) or f"recurso_{s_idx}"
                # Tentativa de extensão ou nome base
                target_file = os.path.join(sec_dir, sanitize_filename(res_label))
                self.download_file(res_url, target_file)

            # 2. Pastas (Folders)
            folders = sec.find_all("a", href=re.compile(r'/mod/folder/view\.php\?id=(\d+)'))
            for f_a in folders:
                folder_url = f_a["href"]
                f_match = re.search(r'id=(\d+)', folder_url)
                if not f_match:
                    continue
                folder_id = f_match.group(1)
                folder_name = f_a.get_text(strip=True) or f"Pasta_{folder_id}"
                folder_target_dir = os.path.join(sec_dir, sanitize_filename(folder_name))
                
                # Tenta descarregar via endpoint nativo do Moodle que gera ZIP da pasta
                zip_url = f"{BASE_URL}/mod/folder/download_folder.php?id={folder_id}"
                try:
                    r_zip = self.session.get(zip_url, stream=True, timeout=30)
                    if r_zip.status_code == 200 and "zip" in r_zip.headers.get("Content-Type", "").lower():
                        import zipfile
                        import io
                        with zipfile.ZipFile(io.BytesIO(r_zip.content)) as z:
                            z.extractall(folder_target_dir)
                        log(f"  [+] Pasta sincronizada (ZIP): {folder_name}")
                        continue
                except Exception:
                    pass

                # Fallback: abrir página da pasta e raspar ficheiros
                try:
                    r_f = self.session.get(folder_url, timeout=20)
                    if r_f.status_code == 200:
                        f_soup = BeautifulSoup(r_f.text, "html.parser")
                        for f_link in f_soup.find_all("a", href=re.compile(r'pluginfile\.php')):
                            f_file_name = f_link.get_text(strip=True) or "anexo"
                            f_path = os.path.join(folder_target_dir, sanitize_filename(f_file_name))
                            self.download_file(f_link["href"], f_path)
                except Exception as e:
                    log(f"  [!] Erro ao processar pasta {folder_name}: {e}")

            # 3. Enunciados de Tarefas/Trabalhos (Assign)
            assigns = sec.find_all("a", href=re.compile(r'/mod/assign/view\.php\?id=\d+'))
            for ass_a in assigns:
                ass_url = ass_a["href"]
                ass_name = ass_a.get_text(strip=True) or "Trabalho"
                try:
                    r_ass = self.session.get(ass_url, timeout=20)
                    if r_ass.status_code == 200:
                        ass_soup = BeautifulSoup(r_ass.text, "html.parser")
                        intro_files = ass_soup.find_all("a", href=re.compile(r'pluginfile\.php'))
                        for ifile in intro_files:
                            fname = ifile.get_text(strip=True) or "enunciado"
                            target_ass_path = os.path.join(sec_dir, "Trabalhos", sanitize_filename(ass_name), sanitize_filename(fname))
                            self.download_file(ifile["href"], target_ass_path)
                except Exception:
                    pass

            # 3b. Páginas (texto no próprio Moodle: enunciados, avisos, guias)
            for pg_a in sec.find_all("a", href=re.compile(r'/mod/page/view\.php\?id=\d+')):
                self.save_page(pg_a["href"], pg_a.get_text(strip=True) or "Pagina", os.path.join(sec_dir, "Paginas"))

            # 4. Links externos (URLs do Moodle)
            url_links = sec.find_all("a", href=re.compile(r'/mod/url/view\.php\?id=\d+'))
            for u_a in url_links:
                u_url = u_a["href"]
                u_title = u_a.get_text(strip=True) or "link"
                try:
                    r_u = self.session.get(u_url, timeout=15, allow_redirects=True)
                    target_url = r_u.url
                    # Cria atalho .url do Windows
                    shortcut_dir = os.path.join(sec_dir, "Links")
                    os.makedirs(shortcut_dir, exist_ok=True)
                    shortcut_path = os.path.join(shortcut_dir, f"{sanitize_filename(u_title)}.url")
                    if not os.path.exists(shortcut_path):
                        with open(shortcut_path, "w", encoding="utf-8") as sf:
                            sf.write(f"[InternetShortcut]\nURL={target_url}\n")
                except Exception:
                    pass

    def run_all(self):
        """Inicia a sincronização de todas as disciplinas."""
        lock = str(paths.sync_lock(self.sync_dir))
        os.makedirs(os.path.dirname(lock), exist_ok=True)
        if os.path.exists(lock) and time.time() - os.path.getmtime(lock) < 3600:
            log("Já existe uma sincronização em curso (sync.lock). A ignorar esta execução.")
            self.busy = True
            return False
        with open(lock, "w") as f:
            f.write(str(os.getpid()))
        try:
            return self._run_all_locked()
        finally:
            try:
                os.remove(lock)
            except OSError:
                pass

    def _run_all_locked(self):
        if not self.login():
            log("Falha ao iniciar sessão. A abortar sincronização.")
            return False

        courses = self.get_enrolled_courses()
        if not courses:
            log("Nenhuma disciplina encontrada.")
            return False

        log(f"Iniciando sincronização para {len(courses)} disciplinas em '{self.download_dir}'...")
        for c in courses:
            try:
                self.sync_course(c["id"], c["name"])
            except Exception as e:  # uma disciplina com erro não impede as restantes
                self.errors += 1
                log(f"  [!] Erro na disciplina {c['name']}: {e}")

        log(f"Sincronização terminada: {len(self.new_files)} ficheiros novos/atualizados, {self.errors} erros.")
        return self.errors == 0

# --- GESTÃO DE CONFIGURAÇÃO E INSTALAÇÃO ---
def create_startup_task():
    """Cria o ficheiro na pasta de arranque do Windows (shell:startup) para correr sem popup."""
    startup_dir = os.path.join(os.environ.get("APPDATA", ""), "Microsoft", "Windows", "Start Menu", "Programs", "Startup")
    if not os.path.exists(startup_dir):
        log(f"Pasta de arranque não encontrada: {startup_dir}")
        return False

    vbs_path = os.path.join(startup_dir, "MoodleISEP_AutoSync.vbs")
    pythonw_path = os.path.join(os.path.dirname(sys.executable), "pythonw.exe")
    if not os.path.exists(pythonw_path):
        pythonw_path = sys.executable

    script_path = str(paths.sync_dir() / "moodle_sync.py")  # atalho que chama este módulo
    
    # Cria ficheiro VBScript que lança o script em background silencioso (0 = janela invisível)
    vbs_content = f'''Set WshShell = CreateObject("WScript.Shell")
WshShell.Run """{pythonw_path}"" ""{script_path}"" --on-startup", 0, False
'''
    try:
        with open(vbs_path, "w", encoding="utf-8") as f:
            f.write(vbs_content)
        log(f"Agendamento de arranque configurado em: {vbs_path}")
        return True
    except Exception as e:
        log(f"Erro ao criar atalho de arranque: {e}")
        return False

def setup_wizard():
    """Assistente interativo de configuração inicial."""
    print("=" * 60)
    print("   CONFIGURAÇÃO - SINCRONIZADOR MOODLE ISEP")
    print("=" * 60)
    print("As tuas credenciais são guardadas de forma 100% segura no teu PC")
    print("usando a tecnologia de encriptação DPAPI nativa do Windows.\n")

    user = input("Número mecanográfico / Utilizador ISEP: ").strip()
    user = clean_username(user)

    import getpass
    password = getpass.getpass("Palavra-passe do Moodle: ").strip()

    print(f"\nPasta onde guardar os materiais (Predefinição: {DEFAULT_DOWNLOAD_DIR})")
    custom_dir = input("Pressiona ENTER para usar a predefinição ou escreve outro caminho: ").strip()
    download_dir = custom_dir if custom_dir else DEFAULT_DOWNLOAD_DIR
    os.makedirs(download_dir, exist_ok=True)

    print("\nA testar ligação ao Moodle do ISEP...")
    sync_engine = MoodleISEPSync(user, password, download_dir)
    if not sync_engine.login():
        print("\n[ERRO] Não foi possível entrar no Moodle com esses dados.")
        print("Verifica se o número e palavra-passe estão corretos.")
        return False

    print("\n[SUCESSO] Credenciais verificadas e válidas!")

    # Guarda configuração com palavra-passe encriptada via DPAPI
    config = {
        "username": user,
        "encrypted_password": encrypt_dpapi(password),
        "download_dir": download_dir,
        "run_after_hour": 18,
        "last_sync_date": ""
    }
    save_config(config)

    # Cria arranque automático
    create_startup_task()

    print("\n" + "=" * 60)
    print("Configuração concluída!")
    print(f"- Todos os dias depois das 18h, quando ligares o PC, o download")
    print(f"  será feito automaticamente em segundo plano para:")
    print(f"  {download_dir}")
    print("=" * 60)

    resp = input("\nQueres fazer uma sincronização de teste agora mesmo? (s/n): ").strip().lower()
    if resp in ["s", "sim", "y", "yes"]:
        sync_engine.run_all()
    return True

@dataclass
class SyncResult:
    ok: bool = False
    busy: bool = False
    new_files: int = 0
    errors: int = 0
    error: Optional[str] = None
    files: List[dict] = field(default_factory=list)

    def to_dict(self) -> dict:
        return asdict(self)


def run_sync(trigger: str = "manual", progress: Optional[Callable[[str], None]] = None) -> SyncResult:
    """Sincroniza todas as disciplinas com a configuração guardada, atualiza last_sync_date e regista no histórico."""
    global _log_sink
    res = SyncResult()
    started = datetime.now().isoformat(timespec="seconds")
    config = load_config()
    if not config or "username" not in config or "encrypted_password" not in config:
        res.error = "Configuração em falta: define o utilizador e a palavra-passe do Moodle."
        log(res.error)
        return res

    previous_sink, _log_sink = _log_sink, progress
    try:
        password = decrypt_dpapi(config["encrypted_password"])
        engine = MoodleISEPSync(config["username"], password, config.get("download_dir", DEFAULT_DOWNLOAD_DIR))
        res.ok = bool(engine.run_all())
        res.busy = engine.busy
        res.new_files = len(engine.new_files)
        res.errors = engine.errors
        res.files = engine.new_files[:100]
        if res.ok:
            config["last_sync_date"] = date.today().isoformat()
            save_config(config)
        elif not res.busy:
            res.error = f"{engine.errors} erro(s) durante a sincronização" if engine.errors else "Falha ao iniciar sessão ou sem disciplinas."
    except Exception as e:
        res.error = str(e)
        log(f"Erro durante a sincronização: {e}")
    finally:
        _log_sink = previous_sink

    if not res.busy:  # uma execução ignorada por haver outra a decorrer não é uma execução
        history.append(history.new_record(
            "moodle_sync", trigger, res.ok, started, error=res.error,
            details={"new_files": res.new_files, "errors": res.errors,
                     "files": [{"course": f.get("course"), "name": f.get("name"), "kind": f.get("kind")} for f in res.files[:40]]}))
    return res


def run_on_startup():
    """Modo executado ao ligar o PC / iniciar sessão no Windows."""
    config = load_config()
    if not config or "username" not in config or "encrypted_password" not in config:
        log("Configuração em falta. Executa primeiro o setup.")
        return

    run_after_hour = config.get("run_after_hour", 18)
    today_str = date.today().isoformat()

    # Verifica se já sincronizou hoje com sucesso
    if config.get("last_sync_date") == today_str:
        log(f"Já foi feita a sincronização hoje ({today_str}). Nada a fazer.")
        return

    now = datetime.now()
    log(f"Verificação de arranque: Hora atual = {now.strftime('%H:%M:%S')}. Hora alvo = {run_after_hour}:00")

    # Se ainda não são 18h (ex: ligou o PC de manhã):
    if now.hour < run_after_hour:
        # Espera em background até às 18h (verificando a cada 5 minutos se já atingiu a hora)
        target_time = now.replace(hour=run_after_hour, minute=0, second=0, microsecond=0)
        seconds_to_wait = (target_time - now).total_seconds()
        log(f"Ainda não são {run_after_hour}h. Aguardando em background ({seconds_to_wait/3600:.1f} horas)...")

        while datetime.now().hour < run_after_hour:
            time.sleep(300)  # Dorme 5 minutos e verifica de novo

    # Chegou a hora (ou o PC foi ligado depois das 18h)
    log("Iniciando sincronização agendada das 18h...")
    if not wait_for_internet():
        log("Sem ligação ao Moodle (DNS). A tentar novamente no próximo arranque.")
        return
    if run_sync(trigger="scheduled").ok:
        log("Sincronização diária concluída e registada.")


def run_manual():
    """Modo manual para forçar sincronização imediata."""
    config = load_config()
    if not config or "username" not in config or "encrypted_password" not in config:
        print("Configuração não encontrada! Por favor executa primeiro o setup.")
        setup_wizard()
        return
    run_sync(trigger="manual")


# --- PONTO DE ENTRADA ---
def main(argv: Optional[List[str]] = None) -> None:
    argv = sys.argv[1:] if argv is None else argv
    if "--setup" in argv:
        setup_wizard()
    elif "--on-startup" in argv:
        run_on_startup()
    elif "--run" in argv or not argv:
        if not paths.sync_config().exists():
            setup_wizard()
        else:
            run_manual()


if __name__ == "__main__":
    main()
