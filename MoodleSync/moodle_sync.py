"""Ponto de entrada do sincronizador do Moodle. A lógica está em isep_assistant.moodle_sync."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from isep_assistant.moodle_sync import main  # noqa: E402

if __name__ == "__main__":
    main()
