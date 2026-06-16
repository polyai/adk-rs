from contextlib import contextmanager
from pathlib import Path
import json
import os
import tempfile
import unittest

# for local development of editable installs
try:
    import maturin_import_hook
except ModuleNotFoundError:
    maturin_import_hook = None

if maturin_import_hook is not None:
    maturin_import_hook.install()

from poly_adk import AdkError, Project, __version__


REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURE_PROJECT = REPO_ROOT / "adk-cli/tests/fixtures/test_projects/test_project"
BASELINE_TOPIC = "name: Welcome\ncontent: Hello there\n"
AUTH_ENV_VARS = [
    "POLY_ADK_KEY",
    "POLY_ADK_KEY_US",
    "POLY_ADK_KEY_EUW",
    "POLY_ADK_KEY_UK",
    "POLY_ADK_KEY_STUDIO",
    "POLY_ADK_KEY_STAGING",
    "POLY_ADK_KEY_DEV",
]


@contextmanager
def without_poly_credentials():
    previous_env = {name: os.environ.get(name) for name in AUTH_ENV_VARS}
    previous_home = os.environ.get("HOME")
    previous_userprofile = os.environ.get("USERPROFILE")
    with tempfile.TemporaryDirectory() as tmp:
        for name in AUTH_ENV_VARS:
            os.environ.pop(name, None)
        os.environ["HOME"] = tmp
        os.environ.pop("USERPROFILE", None)
        try:
            yield
        finally:
            for name, value in previous_env.items():
                if value is None:
                    os.environ.pop(name, None)
                else:
                    os.environ[name] = value
            if previous_home is None:
                os.environ.pop("HOME", None)
            else:
                os.environ["HOME"] = previous_home
            if previous_userprofile is None:
                os.environ.pop("USERPROFILE", None)
            else:
                os.environ["USERPROFILE"] = previous_userprofile


class PackageSmokeTest(unittest.TestCase):
    def test_project_open_exposes_typed_local_operations(self) -> None:
        project = Project.open(str(FIXTURE_PROJECT), api_key="dummy")

        self.assertEqual(project.config.project_id, "test_project")
        self.assertEqual(project.config.account_id, "test_account")
        self.assertTrue(project.root.endswith("test_project"))

        status = project.status()
        self.assertIsInstance(status.modified_files, list)
        self.assertIsInstance(status.new_files, list)
        self.assertTrue(status.has_changes)

        validation = project.validate()
        self.assertTrue(validation.valid)
        self.assertEqual(validation.errors, [])

    def test_local_operations_do_not_require_credentials(self) -> None:
        with without_poly_credentials():
            project = Project.open(str(FIXTURE_PROJECT))

            self.assertEqual(project.config.project_id, "test_project")
            self.assertTrue(project.status().has_changes)
            self.assertTrue(project.validate().valid)

    def test_missing_project_raises_typed_adk_error(self) -> None:
        with self.assertRaises(AdkError) as raised:
            Project.open("/tmp/poly-adk-missing-project", api_key="dummy")

        self.assertEqual(raised.exception.code, "INVALID_PROJECT")
        self.assertIn("No project configuration found", raised.exception.message)

    def test_version_is_exported(self) -> None:
        self.assertEqual(__version__, "0.0.11")

    def test_diff_reports_modified_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            project_dir = Path(tmp) / "project"
            replay_dir = Path(tmp) / "replay"
            project_dir.mkdir()
            replay_dir.mkdir()
            (project_dir / "project.yaml").write_text(
                "region: us-1\n"
                "account_id: test_account\n"
                "project_id: test_project\n"
                "branch_id: main\n",
                encoding="utf-8",
            )
            (project_dir / "topics").mkdir()
            topic_path = project_dir / "topics/welcome.yaml"
            topic_path.write_text(BASELINE_TOPIC, encoding="utf-8")
            (replay_dir / "main.json").write_text(
                json.dumps(
                    {
                        "topics/welcome.yaml": {
                            "resource_id": "topic-1",
                            "name": "Welcome",
                            "file_path": "topics/welcome.yaml",
                            "payload": {"content": BASELINE_TOPIC},
                        }
                    }
                ),
                encoding="utf-8",
            )

            topic_path.write_text(
                "name: Welcome\ncontent: Changed by Python smoke test.\n",
                encoding="utf-8",
            )

            previous_replay_dir = os.environ.get("POLY_ADK_REPLAY_STATE_DIR")
            # Internal Rust ADK test hook, not public Python API: this lets
            # service.diff() use a local replay baseline instead of the remote
            # API so the smoke test stays offline.
            os.environ["POLY_ADK_REPLAY_STATE_DIR"] = str(replay_dir)
            try:
                diff = Project.open(str(project_dir), api_key="dummy").diff(
                    files=["topics/welcome.yaml"]
                )
            finally:
                if previous_replay_dir is None:
                    os.environ.pop("POLY_ADK_REPLAY_STATE_DIR", None)
                else:
                    os.environ["POLY_ADK_REPLAY_STATE_DIR"] = previous_replay_dir

            self.assertEqual(len(diff), 1)
            self.assertEqual(diff.diffs[0].path, "topics/welcome.yaml")
            self.assertIn("Changed by Python smoke test", diff.diffs[0].diff)


if __name__ == "__main__":
    unittest.main()
