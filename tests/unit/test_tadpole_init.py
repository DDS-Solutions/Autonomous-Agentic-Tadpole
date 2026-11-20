"""
@docs ARCHITECTURE:Quality:Verification

### AI Assist Note
**Tadpole Init Unit Tests**: Verifies Day-1 workspace bootstrapper across secure 64-hex
token generation, stdout secret masking (zero raw leakage in `main()`), embedding-model
filtering, live Docker daemon probing, invalid preset rejection, and `--force` overwrite.

### 🔍 Debugging & Observability
- **Failure Path**: Weak token length, raw secret stdout leak, embedding model selected, or `.env` clobber.
- **Telemetry Link**: Search `[test_tadpole_init]` in test logs.
"""

import io
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(REPO_ROOT))

from execution.tadpole_init import (
    initialize_workspace,
    is_docker_daemon_running,
    is_embedding_model,
    main,
    mask_token,
    select_chat_model,
)


class TestTadpoleInit(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_happy_path_creates_hardened_env_and_dirs(self) -> None:
        res = initialize_workspace(
            workspace=self.root,
            preset="hybrid",
            tiered_tokens=True,
            force=False,
            probe_local=False,
        )
        self.assertEqual(res["status"], "created")
        env_text = (self.root / ".env").read_text(encoding="utf-8")
        self.assertIn("NEURAL_TOKEN=", env_text)
        self.assertIn("NEURAL_TOKEN_ADMIN=", env_text)
        self.assertIn("NEURAL_TOKEN_READONLY=", env_text)
        self.assertIn("ALLOW_HOST_SKILL_EXECUTION=false", env_text)
        self.assertTrue((self.root / "data").is_dir())
        self.assertTrue((self.root / "logs").is_dir())

        raw_token = next(
            line.split("=", 1)[1]
            for line in env_text.splitlines()
            if line.startswith("NEURAL_TOKEN=")
        )
        self.assertEqual(len(raw_token), 64)
        self.assertNotIn(raw_token, str(res["token_preview"]))

    def test_cli_main_never_echoes_raw_tokens_to_stdout(self) -> None:
        buf = io.StringIO()
        argv = [
            "tadpole_init.py",
            "--workspace",
            str(self.root),
            "--preset",
            "local",
            "--tiered-tokens",
            "--non-interactive",
        ]
        with mock.patch.object(sys, "argv", argv), mock.patch(
            "execution.tadpole_init.probe_ollama", return_value=(True, ["nomic-embed-text:latest", "gemma4:e4b"])
        ), mock.patch("execution.tadpole_init.is_docker_daemon_running", return_value=False):
            with redirect_stdout(buf):
                code = main()

        self.assertEqual(code, 0)
        stdout = buf.getvalue()
        env_text = (self.root / ".env").read_text(encoding="utf-8")
        for key in ("NEURAL_TOKEN=", "NEURAL_TOKEN_ADMIN=", "NEURAL_TOKEN_READONLY="):
            secret_val = next(line.split("=", 1)[1] for line in env_text.splitlines() if line.startswith(key))
            self.assertNotIn(secret_val, stdout, f"{key} leaked to CLI stdout!")
        self.assertIn("OLLAMA_MODEL=gemma4:e4b", env_text)
        self.assertIn("USE_SANDBOX_DOCKER=false", env_text)
        self.assertIn("NEURAL_TOKEN_ADMIN", stdout)

    def test_failure_path_does_not_clobber_existing_env_without_force(self) -> None:
        env_file = self.root / ".env"
        env_file.write_text("NEURAL_TOKEN=existing_preserved_token\n", encoding="utf-8")
        res = initialize_workspace(
            workspace=self.root,
            preset="local",
            force=False,
            probe_local=False,
        )
        self.assertEqual(res["status"], "skipped")
        self.assertIn("existing_preserved_token", env_file.read_text(encoding="utf-8"))

        # With force=True it overwrites cleanly
        forced = initialize_workspace(
            workspace=self.root,
            preset="cloud",
            force=True,
            probe_local=False,
        )
        self.assertEqual(forced["status"], "created")
        self.assertNotIn("existing_preserved_token", env_file.read_text(encoding="utf-8"))

    def test_rejects_invalid_preset_and_filters_embedding_models(self) -> None:
        with self.assertRaises(ValueError):
            initialize_workspace(workspace=self.root, preset="bogus_tier", probe_local=False)
        self.assertTrue(is_embedding_model("nomic-embed-text:latest"))
        self.assertTrue(is_embedding_model("bge-m3:latest"))
        self.assertFalse(is_embedding_model("gemma4:e4b"))
        self.assertEqual(
            select_chat_model(["nomic-embed-text:latest", "mxbai-embed-large"], "gemma4:e4b"),
            "gemma4:e4b",
        )
        self.assertEqual(
            select_chat_model(["nomic-embed-text:latest", "qwen3:8b"], "gemma4:e4b"),
            "qwen3:8b",
        )

    def test_docker_daemon_probe_returns_false_when_daemon_stopped(self) -> None:
        fake_fail = mock.Mock(returncode=1)
        with mock.patch("shutil.which", return_value="/usr/bin/docker"), mock.patch(
            "subprocess.run", return_value=fake_fail
        ):
            self.assertFalse(is_docker_daemon_running())

    def test_edge_case_mask_token_short_and_long(self) -> None:
        self.assertEqual(mask_token("short"), "[REDACTED]")
        self.assertEqual(mask_token("0123456789abcdef0123"), "0123...0123")


if __name__ == "__main__":
    unittest.main()

# Metadata: [test_tadpole_init]
