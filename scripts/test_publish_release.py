"""Release state and file-boundary tests; never contact GitHub or use real assets."""

import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock
import zipfile

from publish_release import publish_release


COMMIT = "a" * 40
REPOSITORY = "fixture/butter-manager"


class FakeGitHub:
    def __init__(self):
        self.current = None
        self.calls = []
        self.tag_commit = COMMIT
        self.annotated = False
        self.api_error = None
        self.upload_error = False
        self.corrupt_upload = False
        self.move_tag_on_upload = False
        self.draft_hidden_by_tag = False
        self.edit_error = False
        self.list_error = False
        self.new_draft_hidden_reads = 0

    def __call__(self, args):
        self.calls.append(args)
        if args[0] == "api":
            if "--paginate" in args:
                if self.list_error:
                    raise subprocess.CalledProcessError(1, args, stderr="gh: Forbidden (HTTP 403)")
                if self.current and self.new_draft_hidden_reads:
                    self.new_draft_hidden_reads -= 1
                    return "[]"
                # gh --paginate writes consecutive page arrays, not a nested array.
                return '[]\n' + json.dumps([self.current] if self.current else [])
            path = args[1]
            if "/git/ref/tags/" in path:
                obj = {"type": "tag", "sha": "b" * 40} if self.annotated else {"type": "commit", "sha": self.tag_commit}
                return json.dumps({"object": obj})
            if "/git/tags/" in path:
                return json.dumps({"object": {"type": "commit", "sha": self.tag_commit}})
            if self.api_error:
                raise subprocess.CalledProcessError(1, args, stderr=self.api_error)
            if self.current is None or (self.draft_hidden_by_tag and self.current["draft"]):
                raise subprocess.CalledProcessError(1, args, stderr="gh: Not Found (HTTP 404)")
            return json.dumps(self.current)
        action = args[1]
        if action == "create":
            assert "--verify-tag" in args and "--draft" in args
            self.current = {"id": 1, "tag_name": args[2], "draft": True, "assets": []}
        elif action == "upload":
            if self.upload_error:
                raise subprocess.CalledProcessError(1, args, stderr="fixture upload interrupted")
            files = args[3:args.index("--repo")]
            for filename in files:
                path = Path(filename)
                asset = {
                    "name": path.name,
                    "size": path.stat().st_size,
                    "digest": "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest(),
                }
                if self.corrupt_upload:
                    asset["digest"] = "sha256:" + "0" * 64
                self.current["assets"] = [a for a in self.current["assets"] if a["name"] != path.name] + [asset]
            if self.move_tag_on_upload:
                self.tag_commit = "c" * 40
        elif action == "edit":
            assert "--draft=false" in args
            if self.edit_error:
                raise subprocess.CalledProcessError(1, args, stderr="fixture publication interrupted")
            self.current["draft"] = False
        else:
            raise AssertionError(args)
        return ""

    def mutations(self):
        return [args[1] for args in self.calls if args[0] == "release"]


class PublishReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="publish-release-test-")
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.github = FakeGitHub()
        self.make_assets()

    def make_assets(self, version="1.0.0", entries=None):
        self.version = version
        self.tag = "v" + version
        self.exe = self.directory / "butter-manager.exe"
        self.exe.write_bytes(b"MZ synthetic executable fixture")
        self.zip = self.directory / f"butter-manager-{self.tag}-windows-x64-portable.zip"
        with zipfile.ZipFile(self.zip, "w") as archive:
            for name, data in (entries or {"butter-manager/butter-manager.exe": self.exe.read_bytes()}).items():
                archive.writestr(name, data)
        self.write_checksums()

    def write_checksums(self):
        self.checksums = self.directory / "SHA256SUMS.txt"
        self.checksums.write_text("".join(
            f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in (self.exe, self.zip)
        ))

    def publish(self, **changes):
        options = dict(
            directory=self.directory, tag=self.tag, version=self.version,
            repository=REPOSITORY, commit=COMMIT, linux_commit=COMMIT,
            create=True, gh=self.github, sleep=Mock(),
        )
        options.update(changes)
        publish_release(**options)

    def test_tag_push_uploads_to_draft_verifies_then_publishes(self):
        self.publish()
        self.assertEqual(self.github.mutations(), ["create", "upload", "edit"])
        self.assertFalse(self.github.current["draft"])
        self.assertEqual(len(self.github.current["assets"]), 3)
        self.assertIn("--generate-notes", next(a for a in self.github.calls if a[:2] == ["release", "create"]))

    def test_prerelease_tag_does_not_publish_as_stable(self):
        self.zip.unlink()
        self.make_assets("1.0.0-rc.1")
        self.publish()
        self.assertIn("--prerelease", next(a for a in self.github.calls if a[:2] == ["release", "create"]))
        self.assertIn("--prerelease=true", next(a for a in self.github.calls if a[:2] == ["release", "edit"]))

    def test_draft_missing_from_tag_endpoint_is_found_and_resumed(self):
        self.github.draft_hidden_by_tag = True
        self.github.current = {"id": 1, "tag_name": self.tag, "draft": True, "assets": []}
        self.publish()
        self.assertEqual(self.github.mutations(), ["upload", "edit"])
        self.assertFalse(self.github.current["draft"])

    def test_new_draft_visibility_delay_retries_reads_without_recreating(self):
        self.github.draft_hidden_by_tag = True
        self.github.new_draft_hidden_reads = 2
        sleep = Mock()
        self.publish(sleep=sleep)
        self.assertEqual(sleep.call_count, 2)
        sleep.assert_called_with(2)
        self.assertEqual(self.github.mutations(), ["create", "upload", "edit"])

    def test_new_draft_visibility_retry_is_bounded_and_never_publishes(self):
        self.github.draft_hidden_by_tag = True
        self.github.new_draft_hidden_reads = 100
        sleep = Mock()
        with self.assertRaisesRegex(ValueError, "newly created Release"):
            self.publish(sleep=sleep)
        self.assertEqual(sleep.call_count, 5)
        self.assertEqual(self.github.mutations(), ["create"])
        self.assertTrue(self.github.current["draft"])

    def test_publication_failure_keeps_verified_draft_for_retry(self):
        self.github.edit_error = True
        with self.assertRaises(subprocess.CalledProcessError):
            self.publish()
        self.assertTrue(self.github.current["draft"])
        self.assertEqual(len(self.github.current["assets"]), 3)
        self.github.edit_error = False
        self.publish()
        self.assertEqual(self.github.mutations().count("create"), 1)
        self.assertFalse(self.github.current["draft"])

    def test_annotated_tag_is_resolved_to_the_verified_commit(self):
        self.github.annotated = True
        self.publish()
        self.assertFalse(self.github.current["draft"])

    def test_interrupted_upload_keeps_draft_and_rerun_finishes_it(self):
        self.github.upload_error = True
        with self.assertRaises(subprocess.CalledProcessError):
            self.publish()
        self.assertTrue(self.github.current["draft"])
        self.assertNotIn("edit", self.github.mutations())
        self.github.upload_error = False
        self.publish()
        self.assertEqual(self.github.mutations().count("create"), 1)
        self.assertFalse(self.github.current["draft"])

    def test_successful_rerun_does_not_mutate_published_assets(self):
        self.publish()
        self.github.calls.clear()
        self.publish()
        self.assertEqual(self.github.mutations(), [])

    def test_different_published_assets_are_not_overwritten(self):
        self.publish()
        self.github.current["assets"][0]["digest"] = "sha256:" + "0" * 64
        self.github.calls.clear()
        with self.assertRaisesRegex(ValueError, "Refusing to replace"):
            self.publish()
        self.assertEqual(self.github.mutations(), [])

    def test_existing_draft_with_unexpected_assets_is_not_published(self):
        self.github.current = {
            "id": 1, "tag_name": self.tag, "draft": True,
            "assets": [{"name": "unrelated-fixture.txt", "size": 1, "digest": "sha256:" + "0" * 64}],
        }
        with self.assertRaisesRegex(ValueError, "unexpected assets"):
            self.publish()
        self.assertTrue(self.github.current["draft"])
        self.assertEqual(self.github.mutations(), [])

    def test_manual_publishing_requires_existing_release(self):
        with self.assertRaisesRegex(ValueError, "existing Release"):
            self.publish(create=False)
        self.assertEqual(self.github.mutations(), [])

    def test_existing_release_upload_preserves_draft_or_published_state(self):
        for draft in (True, False):
            with self.subTest(draft=draft):
                self.github = FakeGitHub()
                self.github.current = {"id": 1, "tag_name": self.tag, "draft": draft, "assets": []}
                self.publish(create=False)
                self.assertEqual(self.github.mutations(), ["upload"])
                self.assertEqual(self.github.current["draft"], draft)

    def test_auth_or_server_errors_do_not_create_a_release(self):
        for error in ("gh: Forbidden (HTTP 403)", "gh: server error (HTTP 500)", "network timeout"):
            with self.subTest(error=error):
                self.github = FakeGitHub()
                self.github.api_error = error
                with self.assertRaises(subprocess.CalledProcessError):
                    self.publish()
                self.assertEqual(self.github.mutations(), [])

    def test_release_listing_failure_is_not_treated_as_missing_release(self):
        self.github.list_error = True
        with self.assertRaises(subprocess.CalledProcessError):
            self.publish()
        self.assertEqual(self.github.mutations(), [])

    def test_wrong_version_or_different_platform_commits_fail_before_github(self):
        for changes in ({"tag": "1.0.0"}, {"tag": "v2.0.0"}, {"linux_commit": "d" * 40}, {"commit": "short"}):
            with self.subTest(changes=changes):
                with self.assertRaises(ValueError):
                    self.publish(**changes)
                self.assertEqual(self.github.calls, [])

    def test_moved_tag_is_rejected_before_release_creation(self):
        self.github.tag_commit = "d" * 40
        with self.assertRaisesRegex(ValueError, "Tag no longer points"):
            self.publish()
        self.assertEqual(self.github.mutations(), [])

    def test_tag_moved_during_upload_is_not_published(self):
        self.github.move_tag_on_upload = True
        with self.assertRaisesRegex(ValueError, "Tag no longer points"):
            self.publish()
        self.assertTrue(self.github.current["draft"])
        self.assertNotIn("edit", self.github.mutations())

    def test_remote_digest_mismatch_leaves_draft(self):
        self.github.corrupt_upload = True
        with self.assertRaisesRegex(ValueError, "Uploaded asset verification failed"):
            self.publish()
        self.assertTrue(self.github.current["draft"])
        self.assertNotIn("edit", self.github.mutations())

    def test_tampered_file_or_duplicate_checksums_fail_before_github(self):
        self.exe.write_bytes(b"changed fixture")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            self.publish()
        self.write_checksums()
        lines = self.checksums.read_text().splitlines()
        self.checksums.write_text(lines[0] + "\n" + lines[0] + "\n")
        with self.assertRaisesRegex(ValueError, "duplicate checksum"):
            self.publish()
        self.assertEqual(self.github.calls, [])

    def test_zip_with_extra_data_or_different_executable_is_rejected(self):
        cases = [
            {"butter-manager/butter-manager.exe": self.exe.read_bytes(), "data/fixture.txt": b"synthetic"},
            {"butter-manager/butter-manager.exe": b"X" * self.exe.stat().st_size},
        ]
        for entries in cases:
            with self.subTest(entries=list(entries)):
                self.make_assets(entries=entries)
                with self.assertRaises(ValueError):
                    self.publish()
                self.assertEqual(self.github.calls, [])

    def test_extra_asset_or_symlink_is_rejected_before_github(self):
        extra = self.directory / "fixture.txt"
        extra.write_text("synthetic")
        with self.assertRaises(ValueError):
            self.publish()
        extra.unlink()
        original = self.exe.read_bytes()
        self.exe.unlink()
        target = self.directory.parent / (self.directory.name + "-link-fixture")
        target.write_bytes(original)
        self.addCleanup(target.unlink)
        self.exe.symlink_to(target)
        with self.assertRaises(ValueError):
            self.publish()
        self.assertEqual(self.github.calls, [])


if __name__ == "__main__":
    unittest.main()
