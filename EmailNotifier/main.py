"""Ponto de entrada do resumo diário (chamado pelo Agendador de Tarefas). A lógica está em isep_assistant.digest."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from isep_assistant.digest import main  # noqa: E402

if __name__ == "__main__":
    sys.exit(main())
