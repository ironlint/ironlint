import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ADAPTERS = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ADAPTERS / "shared/hooks"))
import hook
import process


def result(check="tests", outcome="pass", code=0):
    return {"id": check, "outcome": outcome, "exit_status": code, "stdout": [],
            "stderr": [], "stdout_truncated": False, "stderr_truncated": False, "reason": None}


def verdict(event="accept", status="pass", results=None):
    return {"schema": 7, "event": event, "status": status, "results": results if results is not None else [result()], "not_run": [], "error": None}


class ContractTests(unittest.TestCase):
    def test_schema_exit_and_expected_check_set_must_all_agree(self):
        good = verdict()
        self.assertEqual(hook.classify(json.dumps(good).encode(), 0, "accept", ["tests"])[0], "pass")
        mutations = [dict(schema=8), dict(event="change"), dict(status="violation"), dict(error="timeout"),
                     dict(not_run=[{"id": "tests"}]), dict(results=[]), dict(results=[result(), result()])]
        for mutation in mutations:
            candidate = good | mutation
            self.assertEqual(hook.classify(json.dumps(candidate).encode(), 0, "accept", ["tests"])[0], "incomplete", mutation)
        for mutation in [{"stdout_truncated": True}, {"stderr_truncated": True}, {"stdout": [True]},
                         {"stderr": [256]}, {"exit_status": True}, {"outcome": "error"}, {"id": ""}]:
            candidate = copy.deepcopy(good)
            candidate["results"][0].update(mutation)
            self.assertEqual(hook.classify(json.dumps(candidate).encode(), 0, "accept", ["tests"])[0], "incomplete")
        for raw in [b"", b"{}", b"[]", b"null", b"\xff", b'{"schema":7']:
            self.assertEqual(hook.classify(raw, 0, "accept", ["tests"])[0], "incomplete")
        self.assertEqual(hook.classify(json.dumps(good).encode(), 2, "accept", ["tests"])[0], "incomplete")

    def test_change_empty_selection_is_success_but_never_acceptance(self):
        raw = json.dumps(verdict("change", "not_run", [])).encode()
        self.assertEqual(hook.classify(raw, 0, "change")[0], "pass")
        self.assertEqual(hook.classify(raw, 0, "accept", ["tests"])[0], "incomplete")

    def test_violations_return_bounded_diagnostics_and_continuation(self):
        row = result(outcome="violation", code=1)
        row["stdout"] = list(b"fix the candidate")
        kind, detail = hook.classify(json.dumps(verdict(status="violation", results=[row])).encode(), 2, "accept", ["tests"])
        self.assertEqual(kind, "violation")
        self.assertIn("fix the candidate", detail)
        self.assertEqual(hook.feedback("accept", kind, detail, ["ironlint"])["decision"], "block")
        continued = hook.feedback("accept", kind, detail, ["ironlint"], True)
        self.assertNotIn("decision", continued)
        self.assertIn("incomplete", continued["systemMessage"])
        self.assertNotIn("decision", hook.feedback("change", kind, detail, ["ironlint"]))

    def test_codex_patch_add_delete_update_and_move_include_both_endpoints(self):
        text = "*** Begin Patch\n*** Add File: a.rs\n+x\n*** Update File: b.rs\n*** Move to: c.rs\n@@\n-x\n+y\n*** Delete File: d.rs\n*** End Patch"
        self.assertEqual(hook.patch_paths(text), ["a.rs", "b.rs", "c.rs", "d.rs"])
        for malformed in [None, {}, "not a patch", "*** Begin Patch\n*** Add File: \n*** End Patch", "*** Begin Patch\n*** End Patch"]:
            self.assertIsNone(hook.patch_paths(malformed))

    def test_mutation_path_fallbacks_are_conservative(self):
        root = Path("/project")
        for tool in ["Edit", "Write", "MultiEdit"]:
            self.assertEqual(hook.changed_paths({"tool_name": tool, "tool_input": {"file_path": "/project/a.rs"}}, root), ["a.rs"])
            self.assertIsNone(hook.changed_paths({"tool_name": tool, "tool_input": {}}, root))
        self.assertEqual(hook.changed_paths({"tool_name": "NotebookEdit", "tool_input": {"notebook_path": "a.ipynb"}}, root), ["a.ipynb"])
        self.assertIsNone(hook.changed_paths({"tool_name": "Write", "tool_input": {"file_path": "../outside"}}, root))
        self.assertEqual(hook.changed_paths({"tool_name": "Bash"}, root), [])

    def test_missing_policy_or_invalid_payload_does_not_claim_acceptance(self):
        with tempfile.TemporaryDirectory() as directory:
            output = hook.evaluate({"cwd": directory, "hook_event_name": "Stop"}, "stop")
            self.assertIn("incomplete", output["systemMessage"])
        self.assertIn("unexpected hook event", hook.evaluate({}, "stop")["systemMessage"])
        self.assertIn("absolute directory", hook.evaluate({"hook_event_name": "Stop", "cwd": "."}, "stop")["systemMessage"])

    def test_both_manifests_use_synchronous_post_edit_and_stop(self):
        for harness in ["codex", "claude-code"]:
            data = json.loads((ADAPTERS / harness / "hooks/hooks.json").read_text())
            self.assertEqual(set(data["hooks"]), {"PostToolUse", "Stop"})
            for groups in data["hooks"].values():
                handler = groups[0]["hooks"][0]
                self.assertGreater(handler["timeout"], 600 + process.GRACE_SECONDS)
                self.assertNotIn("async", handler)


class ProcessTests(unittest.TestCase):
    def test_closed_pipes_do_not_hide_an_unfinished_child(self):
        script = "import os,time; os.close(1); os.close(2); time.sleep(60)"
        with self.assertRaisesRegex(RuntimeError, "deadline"):
            process.run([sys.executable, "-c", script], Path.cwd(), time.monotonic() + .1)

    def test_deadline_and_output_caps_stop_owned_processes(self):
        for script, seconds, cap, expected in [("import time; time.sleep(60)", .05, 1024, "deadline"),
                                               ("print('x'*10000)", 5, 32, "output")]:
            with self.assertRaisesRegex(RuntimeError, expected):
                process.run([sys.executable, "-c", script], Path.cwd(), time.monotonic() + seconds, cap)

    def test_stderr_is_drained_and_clipped_without_losing_stdout(self):
        code, stdout, stderr = process.run([sys.executable, "-c", "import sys; sys.stderr.write('x'*1000000); print('ok')"], Path.cwd(), time.monotonic() + 5)
        self.assertEqual((code, stdout), (0, b"ok\n"))
        self.assertEqual(len(stderr), process.STDERR_LIMIT)


@unittest.skipUnless(os.environ.get("IRONLINT_TEST_BIN"), "real CLI supplied by ci-adapters.sh")
class RealCliTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.policy = self.root / ".ironlint.yml"
        self.policy.write_text("version: 1\nchecks:\n  tests:\n    on: [change, accept]\n    files: ['*.rs']\n    run: test -f ready\n")
        self.binary = str(Path(os.environ["IRONLINT_TEST_BIN"]).resolve())
        self.env = dict(os.environ, HOME=str(self.root / "home"), XDG_CONFIG_HOME=str(self.root / "config"), IRONLINT_BIN=self.binary)
        trusted = subprocess.run([self.binary, "trust", "--config", str(self.policy)], input=b"y\n", env=self.env, cwd=self.root, capture_output=True)
        self.assertEqual(trusted.returncode, 0, trusted.stderr)

    def call(self, harness, event, **fields):
        payload = {"cwd": str(self.root), "hook_event_name": event, **fields}
        mode = "stop" if event == "Stop" else "post-tool-use"
        child = subprocess.run(["sh", str(ADAPTERS / harness / "hooks/hook.sh"), mode], input=json.dumps(payload).encode(), cwd=self.root, env=self.env, capture_output=True)
        self.assertEqual(child.returncode, 0, child.stderr)
        return json.loads(child.stdout)

    def test_red_feedback_stop_repair_and_fresh_acceptance_in_both_harnesses(self):
        for harness, tool, fields in [("codex", "apply_patch", {"command": "*** Begin Patch\n*** Add File: a.rs\n+x\n*** End Patch"}),
                                      ("claude-code", "Write", {"file_path": str(self.root / "a.rs")})]:
            with self.subTest(harness=harness):
                (self.root / "ready").unlink(missing_ok=True)
                feedback = self.call(harness, "PostToolUse", tool_name=tool, tool_input=fields)
                self.assertIn("tests:", feedback["hookSpecificOutput"]["additionalContext"])
                self.assertEqual(self.call(harness, "Stop")["decision"], "block")
                self.assertIn("incomplete", self.call(harness, "Stop", stop_hook_active=True)["systemMessage"])
                (self.root / "ready").touch()
                self.assertEqual(self.call(harness, "Stop", stop_hook_active=True), {})
                (self.root / "ready").unlink()
                self.assertEqual(self.call(harness, "Stop")["decision"], "block")

    def test_stop_runs_acceptance_even_when_change_was_not_selected(self):
        self.assertEqual(self.call("claude-code", "PostToolUse", tool_name="Write", tool_input={"file_path": "README.md"}), {})
        self.assertEqual(self.call("claude-code", "Stop")["decision"], "block")

    def test_missing_binary_untrusted_policy_and_background_work_are_incomplete(self):
        self.env["IRONLINT_BIN"] = str(self.root / "missing")
        self.assertIn("incomplete", self.call("codex", "Stop")["systemMessage"])
        self.env["IRONLINT_BIN"] = self.binary
        self.policy.write_text(self.policy.read_text() + "\n# changed\n")
        self.assertIn("incomplete", self.call("codex", "Stop")["systemMessage"])
        output = self.call("claude-code", "Stop", background_tasks=[{"status": "running"}])
        self.assertIn("background", output["systemMessage"])


if __name__ == "__main__":
    unittest.main()
