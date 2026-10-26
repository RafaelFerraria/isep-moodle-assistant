import json
import os

import pytest

from isep_assistant import config_store as cs

SID = "05" + "ab" * 32  # 66 caracteres


def test_public_settings_never_exposes_secrets():
    cfg = {"session_mnemonic": "palavra um dois", "ai_api_key": "sk-segredo", "email_password": "pw",
           "session_recipients": [SID], "summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"}
    pub = cs.public_settings(cfg)
    blob = json.dumps(pub)
    for secret in ("palavra um dois", "sk-segredo", '"pw"', SID):
        assert secret not in blob
    assert pub["summary"]["ai_api_key_set"] is True
    assert pub["destination"]["session_configured"] is True
    assert pub["destination"]["session_recipients"] == [f"{SID[:6]}…{SID[-4:]}"]


def test_defaults_when_config_is_empty(home):
    pub = cs.public_settings({})
    assert pub["summary"]["mode"] == "rules" and pub["summary"]["ai_provider"] == "cli"
    assert pub["schedule"]["time"] == "18:00" and pub["destination"]["session_configured"] is False


def test_env_key_detection(monkeypatch):
    monkeypatch.setenv("ANTHROPIC_API_KEY", "x")
    pub = cs.public_settings({"ai_api_type": "anthropic"})
    assert pub["summary"]["ai_api_key_env"] == "ANTHROPIC_API_KEY" and pub["summary"]["ai_api_key_env_set"] is True


def test_patch_switches_to_ai_cli():
    new, effects = cs.apply_settings_patch({}, {"summary": {"mode": "ai", "ai_provider": "cli", "ai_cli": "gemini"}})
    assert (new["summary_mode"], new["ai_provider"], new["ai_cli"]) == ("ai", "cli", "gemini") and effects == []


def test_patch_collects_all_errors_at_once():
    with pytest.raises(cs.ValidationError) as err:
        cs.apply_settings_patch({}, {"summary": {"mode": "magia", "ai_cli": "x", "ai_base_url": "ftp://a"},
                                     "schedule": {"time": "25:99"}, "destination": {"session_id": "123"}})
    assert set(err.value.errors) == {"summary.mode", "summary.ai_cli", "summary.ai_base_url",
                                     "schedule.time", "destination.session_id"}


def test_custom_cli_and_compatible_api_need_their_fields():
    with pytest.raises(cs.ValidationError) as e1:
        cs.apply_settings_patch({}, {"summary": {"mode": "ai", "ai_provider": "cli", "ai_cli": "custom"}})
    assert "summary.ai_cli_command" in e1.value.errors
    with pytest.raises(cs.ValidationError) as e2:
        cs.apply_settings_patch({}, {"summary": {"mode": "ai", "ai_provider": "api", "ai_api_type": "openai_compatible"}})
    assert {"summary.ai_base_url", "summary.ai_model"} <= set(e2.value.errors)


def test_schedule_normalises_time_and_flags_reschedule_only_on_change():
    new, effects = cs.apply_settings_patch({"scheduled_time": "18:00"}, {"schedule": {"time": "7:05"}})
    assert new["scheduled_time"] == "07:05" and effects == ["reschedule"]
    _, effects2 = cs.apply_settings_patch(new, {"schedule": {"time": "07:05"}})
    assert effects2 == []


def test_api_key_can_be_set_and_cleared_but_not_read():
    new, _ = cs.apply_settings_patch({}, {"summary": {"ai_api_key": " sk-abc "}})
    assert new["ai_api_key"] == "sk-abc"
    cleared, _ = cs.apply_settings_patch(new, {"summary": {"clear_ai_api_key": True}})
    assert "ai_api_key" not in cleared
    untouched, _ = cs.apply_settings_patch(new, {"summary": {"mode": "rules"}})
    assert untouched["ai_api_key"] == "sk-abc"


def test_session_id_sets_both_keys_and_unknown_keys_survive():
    new, _ = cs.apply_settings_patch({"extra": 1, "session_mnemonic": "m"}, {"destination": {"session_id": SID}})
    assert new["session_recipients"] == [SID] and new["session_group_id"] == SID
    assert new["extra"] == 1 and new["session_mnemonic"] == "m"


def test_patch_does_not_mutate_the_original():
    original = {"summary_mode": "rules"}
    cs.apply_settings_patch(original, {"summary": {"mode": "ai", "ai_provider": "cli"}})
    assert original == {"summary_mode": "rules"}


def test_save_and_load_roundtrip_is_atomic(home):
    cs.save_config({"a": "ç", "n": 1})
    assert cs.load_config() == {"a": "ç", "n": 1}
    assert not (home / "EmailNotifier" / "config.json.tmp").exists()


def test_load_tolerates_bom_and_garbage(home):
    path = home / "EmailNotifier" / "config.json"
    path.write_bytes(b"\xef\xbb\xbf" + json.dumps({"k": 1}).encode())
    assert cs.load_config() == {"k": 1}
    path.write_text("{não é json", encoding="utf-8")
    assert cs.load_config() == {}
    path.unlink()
    assert cs.load_config() == {}


def test_recipients_accepts_string_list_and_legacy_group_id():
    assert cs.recipients_of({"session_recipients": SID}) == [SID]
    assert cs.recipients_of({"session_recipients": [SID, " "]}) == [SID]
    assert cs.recipients_of({"session_group_id": SID}) == [SID]
    assert cs.recipients_of({}) == []
