"""CI scope regression tests use synthetic repositories and no GitHub credentials."""
import contextlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import ci_scope


class ScopeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.cwd = contextlib.chdir(self.temp.name)
        self.cwd.__enter__()
        self.addCleanup(self.cwd.__exit__, None, None, None)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.name", "CI fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.write("src/app.ts", "original code")
        self.write("README.md", "original documentation")
        self.base = self.commit()

    def git(self, *args):
        return subprocess.check_output(["git", *args], stderr=subprocess.PIPE).decode().strip()

    def write(self, path, content):
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def commit(self):
        self.git("add", "--all")
        self.git("commit", "-qm", "fixture")
        return self.git("rev-parse", "HEAD")

    def test_only_known_documentation_is_ignored(self):
        for name in ["README.md", "CONTRIBUTING.md", "AGENTS.md", "CHANGELOG.md",
                     "docs/roadmap.md", "docs/diagram.svg", "src/assets/lucide/README.md"]:
            self.assertTrue(ci_scope.documentation(name), name)
        for name in ["docs/check.py", "docs/settings.json", "src/rules.md", "src/main.rs",
                     "scripts/check.sh", "package.json", "LICENSE", ".github/workflows/check.yml"]:
            self.assertFalse(ci_scope.documentation(name), name)

    def test_documentation_push_skips_and_multi_commit_code_push_does_not(self):
        self.write("docs/roadmap.md", "synthetic roadmap")
        doc = self.commit()
        scope = ci_scope.plan("push", {"before": self.base, "after": doc}, "refs/heads/main")
        self.assertFalse(ci_scope.decision(scope, False)[0])
        self.write("src/app.ts", "new code")
        self.commit()
        self.write("README.md", "doc followup")
        head = self.commit()
        scope = ci_scope.plan("push", {"before": doc, "after": head}, "refs/heads/main")
        self.assertTrue(ci_scope.decision(scope, True)[0])

    def test_source_key_ignores_docs_but_includes_build_inputs_and_modes(self):
        key = ci_scope.source_key()
        self.write("docs/roadmap.md", "docs only")
        self.commit()
        self.assertEqual(ci_scope.source_key(), key)
        self.git("rm", "README.md")
        self.commit()
        self.assertEqual(ci_scope.source_key(), key)
        for path in ["src/app.ts", "package.json", ".github/workflows/check.yml", "scripts/ci_scope.py"]:
            self.write(path, "new build input")
            self.commit()
            new = ci_scope.source_key()
            self.assertNotEqual(new, key, path)
            key = new
        self.git("update-index", "--chmod=+x", "src/app.ts")
        self.git("commit", "-qm", "change tracked mode")
        self.assertNotEqual(ci_scope.source_key(), key)

    def test_code_renamed_into_docs_still_requires_checks(self):
        Path("docs").mkdir()
        self.git("mv", "src/app.ts", "docs/example.txt")
        head = self.commit()
        self.assertFalse(ci_scope.docs_between(self.base, head))

    def test_missing_zero_or_unknown_history_never_skips(self):
        for before in [None, "0" * 40, "1" * 40, "HEAD", "--help", "bad\nvalue"]:
            scope = ci_scope.plan("push", {"before": before, "after": self.base}, "refs/heads/main")
            self.assertTrue(ci_scope.decision(scope, True)[0], before)
        self.assertFalse(ci_scope.docs_between(self.base, self.base))

    def test_mixed_pr_documentation_followup_needs_matching_success_marker(self):
        self.write("src/app.ts", "PR code")
        code = self.commit()
        code_key = ci_scope.source_key()
        self.write("docs/roadmap.md", "PR docs")
        head = self.commit()
        event = {"action": "synchronize", "before": code,
                 "pull_request": {"base": {"sha": self.base}, "head": {"sha": head}}}
        scope = ci_scope.plan("pull_request", event, "refs/pull/1/merge")
        self.assertFalse(scope["docs_only"])
        self.assertTrue(scope["docs_commit"])
        self.assertEqual(scope["source_key"], code_key)
        self.assertTrue(ci_scope.decision(scope, False)[0])  # Pending, failed, cancelled, evicted or cold.
        self.assertFalse(ci_scope.decision(scope, True)[0])

    def test_new_or_reopened_mixed_pr_cannot_reuse_marker_to_skip_code_checks(self):
        self.write("src/app.ts", "PR code")
        head = self.commit()
        for action in ["opened", "reopened", "ready_for_review"]:
            scope = ci_scope.plan("pull_request", {"action": action,
                "pull_request": {"base": {"sha": self.base}, "head": {"sha": head}}}, "refs/pull/1/merge")
            self.assertTrue(ci_scope.decision(scope, True)[0])

    def test_documentation_pr_uses_merge_base_not_unrelated_base_branch_changes(self):
        self.git("checkout", "-qb", "documentation")
        self.write("docs/roadmap.md", "PR documentation")
        head = self.commit()
        self.git("checkout", "-q", "main")
        self.write("src/app.ts", "upstream code")
        base = self.commit()
        self.git("merge", "-qm", "synthetic PR merge", "documentation")
        scope = ci_scope.plan("pull_request", {"action": "opened",
            "pull_request": {"base": {"sha": base}, "head": {"sha": head}}}, "refs/pull/1/merge")
        self.assertFalse(ci_scope.decision(scope, False)[0])

    def test_actual_merge_source_changes_cache_key_when_base_advances(self):
        self.git("checkout", "-qb", "feature")
        self.write("src/feature.ts", "feature")
        self.commit()
        old = ci_scope.source_key()
        self.git("checkout", "-q", "main")
        self.write("src/app.ts", "new base")
        self.commit()
        self.git("merge", "-qm", "synthetic PR merge", "feature")
        self.assertNotEqual(ci_scope.source_key(), old)

    def test_manual_tag_release_and_unknown_events_always_build_even_with_a_marker(self):
        for event, ref in [("push", "refs/tags/v1.4.0"), ("release", "refs/tags/v1.4.0"),
                           ("workflow_dispatch", "refs/heads/main"), ("unknown", "refs/heads/main")]:
            scope = ci_scope.plan(event, {}, ref)
            self.assertTrue(scope["forced"])
            self.assertTrue(ci_scope.decision(scope, True)[0])

    def test_names_with_unicode_spaces_or_newlines_are_not_shell_commands(self):
        self.write("docs/中文 space\nfile.md", "fixture")
        head = self.commit()
        self.assertTrue(ci_scope.docs_between(self.base, head))
        self.write("src/中文 space\nfile.ts", "fixture")
        head = self.commit()
        self.assertFalse(ci_scope.docs_between(self.base, head))

    def test_cli_outputs_and_summary_explain_skipped_artifact(self):
        self.write("README.md", "docs")
        head = self.commit()
        Path("event.json").write_text(json.dumps({"before": self.base, "after": head}))
        env = {"GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/main",
               "GITHUB_EVENT_PATH": "event.json", "GITHUB_OUTPUT": "output", "GITHUB_STEP_SUMMARY": "summary"}
        with patch.dict(os.environ, env), patch("sys.argv", ["ci_scope.py", "plan", "--plan-file", "plan.json"]):
            ci_scope.main()
        with patch.dict(os.environ, env), patch("sys.argv", ["ci_scope.py", "decide", "--plan-file", "plan.json"]):
            ci_scope.main()
        self.assertIn("run_checks=false", Path("output").read_text())
        self.assertIn("No new EXE", Path("summary").read_text())


if __name__ == "__main__":
    unittest.main()
