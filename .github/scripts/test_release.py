import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import release


class VersionTests(unittest.TestCase):
    def test_manifest_and_lock_validation(self):
        cargo = '[package]\nversion = "1.2.3"'
        lock = '[[package]]\nname = "app"\nversion = "1.2.3"'
        self.assertEqual(release.validate_versions(cargo, '{"version":"1.2.3"}', lock), "1.2.3")
        for package, locked in [('{"version":"1.2.4"}', lock), ('{"version":"1.2.3"}', lock.replace('1.2.3', '1.2.2'))]:
            with self.assertRaises(ValueError):
                release.validate_versions(cargo, package, locked)

    def test_version_comparison(self):
        self.assertTrue(release.increased("1.10.0", "1.9.9"))
        self.assertFalse(release.increased("1.2.3", "1.2.3"))
        with self.assertRaises(ValueError):
            release.increased("1.2.2", "1.2.3")
        for invalid in ["v1.2.3", "01.2.3", "1.2", "1.2.3-beta.1", "1.2.3\n"]:
            with self.assertRaises(ValueError):
                release.version_tuple(invalid)

    def test_gate_uses_push_base_not_parent(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory, "event.json")
            output = Path(directory, "output")
            event.write_text(json.dumps({"before": "push-base"}))
            env = {"GITHUB_EVENT_PATH": str(event), "GITHUB_OUTPUT": str(output), "GITHUB_EVENT_NAME": "push", "GITHUB_SHA": "head", "GITHUB_REPOSITORY": "owner/repo"}
            with patch.dict(os.environ, env), patch.object(release, "validate_versions", return_value="0.2.0"), patch.object(release, "previous_version", return_value="0.1.0") as previous, patch.object(release, "verify_tag", return_value=False), patch.object(release, "existing_release", return_value=None):
                release.gate()
                previous.assert_called_once_with("push-base")
            self.assertIn("release=true", output.read_text())

    def test_unchanged_push_skips_packaging(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory, "event.json")
            output = Path(directory, "output")
            event.write_text(json.dumps({"before": "push-base"}))
            env = {"GITHUB_EVENT_PATH": str(event), "GITHUB_OUTPUT": str(output), "GITHUB_EVENT_NAME": "push"}
            with patch.dict(os.environ, env), patch.object(release, "validate_versions", return_value="0.2.0"), patch.object(release, "previous_version", return_value="0.2.0"), patch.object(release, "existing_release") as lookup:
                release.gate()
                lookup.assert_not_called()
            self.assertIn("release=false", output.read_text())

    def test_manual_run_can_publish_initial_version(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory, "event.json")
            output = Path(directory, "output")
            event.write_text("{}")
            env = {"GITHUB_EVENT_PATH": str(event), "GITHUB_OUTPUT": str(output), "GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_SHA": "head", "GITHUB_REPOSITORY": "owner/repo"}
            with patch.dict(os.environ, env), patch.object(release, "validate_versions", return_value="0.2.0"), patch.object(release, "verify_tag", return_value=False), patch.object(release, "existing_release", return_value=None):
                release.gate()
            self.assertIn("release=true", output.read_text())

    def test_missing_push_base_is_fetched_before_reading_manifest(self):
        missing = subprocess.CompletedProcess([], 1)
        with patch.object(subprocess, "run", return_value=missing), patch.object(release, "run", side_effect=["", '[package]\nversion="0.3.3"']) as run:
            self.assertEqual(release.previous_version("old-sha"), "0.3.3")
        self.assertEqual([call.args for call in run.call_args_list], [
            ("git", "-c", "credential.helper=", "-c", "credential.helper=!gh auth git-credential", "fetch", "--no-tags", "origin", "old-sha"),
            ("git", "show", "old-sha:app/src-tauri/Cargo.toml"),
        ])

    def test_available_push_base_does_not_fetch(self):
        present = subprocess.CompletedProcess([], 0)
        with patch.object(subprocess, "run", return_value=present), patch.object(release, "run", return_value='[package]\nversion="0.3.3"') as run:
            self.assertEqual(release.previous_version("old-sha"), "0.3.3")
            run.assert_called_once_with("git", "show", "old-sha:app/src-tauri/Cargo.toml")

    def test_push_base_fetch_failure_is_not_ignored(self):
        with patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 1)), patch.object(release, "run", side_effect=subprocess.CalledProcessError(128, "git fetch")), self.assertRaises(subprocess.CalledProcessError):
            release.previous_version("old-sha")

    def test_missing_manifest_is_not_treated_as_initial_release(self):
        with patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), patch.object(release, "run", side_effect=subprocess.CalledProcessError(128, "git show")), self.assertRaises(subprocess.CalledProcessError):
            release.previous_version("old-sha")

    def test_conflicting_tag_fails(self):
        result = subprocess.CompletedProcess([], 0, stdout="other-commit\n")
        with patch.object(subprocess, "run", return_value=result), self.assertRaises(ValueError):
            release.verify_tag("v1.2.3", "built-commit")


class PublishTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        for name in release.expected_assets("1.2.3"):
            Path(self.directory.name, name).write_bytes(b"installer")
        self.env = patch.dict(os.environ, {"GITHUB_REPOSITORY": "owner/repo", "GITHUB_SHA": "built-commit"})
        self.env.start()
        self.addCleanup(self.env.stop)

    def publish(self, existing):
        complete = {"assets": [{"name": n} for n in [
            "Agentic.Orchestrator_1.2.3_x64-setup.exe",
            "Agentic.Orchestrator_1.2.3_aarch64.dmg",
            "Agentic.Orchestrator_1.2.3_x64.dmg",
        ]]}
        with patch.object(release, "run") as run, patch.object(release, "verify_tag", return_value=False), patch.object(release, "existing_release", side_effect=[existing, complete]):
            release.publish("1.2.3", self.directory.name)
            return [call.args for call in run.call_args_list]

    def test_github_normalizes_spaces_in_asset_names(self):
        self.assertEqual(release.expected_uploaded_assets("0.3.0"), {
            "Agentic.Orchestrator_0.3.0_x64-setup.exe",
            "Agentic.Orchestrator_0.3.0_aarch64.dmg",
            "Agentic.Orchestrator_0.3.0_x64.dmg",
        })
        self.assertIn("Agentic Orchestrator_0.3.0_aarch64.dmg", release.expected_assets("0.3.0"))

    def test_publish_only_after_upload(self):
        calls = self.publish(None)
        self.assertEqual([c[2] for c in calls if c[0] == "gh"], ["create", "upload", "edit"])
        self.assertIn("--draft", calls[1])
        self.assertIn("--generate-notes", calls[1])

    def test_retry_draft_without_materialized_tag(self):
        calls = self.publish({"draft": True, "target_commitish": "built-commit"})
        self.assertEqual([c[2] for c in calls if c[0] == "gh"], ["upload", "edit"])

    def test_published_release_is_untouched(self):
        with patch.object(release, "run") as run, patch.object(release, "verify_tag", return_value=True), patch.object(release, "existing_release", return_value={"draft": False}):
            release.publish("1.2.3", self.directory.name)
            self.assertEqual(run.call_count, 1)  # Only fetch tags.

    def test_wrong_commit_draft_is_rejected(self):
        with self.assertRaises(ValueError):
            self.publish({"draft": True, "target_commitish": "other-commit"})

    def test_incomplete_uploaded_assets_keep_draft(self):
        with patch.object(release, "run") as run, patch.object(release, "verify_tag", return_value=False), patch.object(release, "existing_release", side_effect=[None, {"assets": []}]), self.assertRaises(ValueError):
            release.publish("1.2.3", self.directory.name)
        self.assertNotIn("edit", [c.args[2] for c in run.call_args_list if c.args[0] == "gh"])

    def test_missing_installer_blocks_publication(self):
        next(Path(self.directory.name).iterdir()).unlink()
        with patch.object(release, "run") as run, self.assertRaises(ValueError):
            release.publish("1.2.3", self.directory.name)
        run.assert_not_called()

    def test_upload_failure_keeps_draft(self):
        def command(*args):
            if args[:3] == ("gh", "release", "upload"):
                raise subprocess.CalledProcessError(1, args)
        with patch.object(release, "run", side_effect=command) as run, patch.object(release, "verify_tag", return_value=False), patch.object(release, "existing_release", return_value=None), self.assertRaises(subprocess.CalledProcessError):
            release.publish("1.2.3", self.directory.name)
        self.assertNotIn("edit", [c.args[2] for c in run.call_args_list if c.args[0] == "gh"])


if __name__ == "__main__":
    unittest.main()
