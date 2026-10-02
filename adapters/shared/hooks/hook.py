"""Shared v1 native-hook feedback. No persistent approval or repair state."""
import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import sys
import time

from process import run

MUTATIONS = {"apply_patch", "Edit", "Write", "MultiEdit", "NotebookEdit"}


def patch_paths(command):
    if not isinstance(command, str):
        return None
    lines = command.strip().splitlines()
    if not lines or lines[0] != "*** Begin Patch" or lines[-1] != "*** End Patch":
        return None
    paths = []
    for line in lines:
        for prefix in ("*** Add File: ", "*** Update File: ", "*** Delete File: ", "*** Move to: "):
            if line.startswith(prefix):
                path = line[len(prefix):]
                if not path.strip():
                    return None
                paths.append(path)
    return list(dict.fromkeys(paths)) or None


def changed_paths(payload, root):
    tool = payload.get("tool_name")
    if tool not in MUTATIONS:
        return []
    fields = payload.get("tool_input")
    fields = fields if isinstance(fields, dict) else {}
    if tool == "apply_patch":
        paths = patch_paths(fields.get("command"))
    else:
        path = fields.get("notebook_path" if tool == "NotebookEdit" else "file_path")
        paths = [path] if isinstance(path, str) and path else None
    if paths is None:
        return None
    normalized = []
    for path in paths:
        try:
            resolved = (root / path).resolve()
            normalized.append(str(resolved.relative_to(root)))
        except (ValueError, OSError):
            return None
    return list(dict.fromkeys(normalized))


def valid_bytes(value):
    return isinstance(value, list) and all(type(byte) is int and 0 <= byte <= 255 for byte in value)


def valid_result(result):
    if not isinstance(result, dict) or not isinstance(result.get("id"), str) or not result["id"]:
        return False
    if not valid_bytes(result.get("stdout")) or not valid_bytes(result.get("stderr")):
        return False
    if result.get("stdout_truncated") is not False or result.get("stderr_truncated") is not False:
        return False
    if "reason" not in result or (result["reason"] is not None and not isinstance(result["reason"], str)):
        return False
    code = result.get("exit_status")
    if type(code) is not int:
        return False
    return ((result.get("outcome") == "pass" and code == 0)
            or (result.get("outcome") == "violation" and 1 <= code <= 125))


def classify(stdout, code, event, expected=None):
    try:
        verdict = json.loads(stdout.decode("utf-8"))
    except (ValueError, UnicodeError):
        return "incomplete", "malformed or truncated IronLint result"
    if (not isinstance(verdict, dict) or type(verdict.get("schema")) is not int
            or verdict["schema"] != 7 or verdict.get("event") != event
            or "error" not in verdict or verdict["error"] is not None
            or verdict.get("not_run") != [] or not isinstance(verdict.get("results"), list)):
        return "incomplete", "IronLint evaluation did not finish completely"
    results = verdict["results"]
    if not all(valid_result(result) for result in results):
        return "incomplete", "invalid or truncated check outcome"
    ids = [result["id"] for result in results]
    if len(set(ids)) != len(ids) or (expected is not None and sorted(ids) != sorted(expected)):
        return "incomplete", "IronLint returned an unexpected check set"
    if code == 0 and verdict.get("status") == "pass" and all(r["outcome"] == "pass" for r in results):
        return "pass", ""
    if code == 0 and event == "change" and verdict.get("status") == "not_run" and not results:
        return "pass", ""
    if code == 2 and verdict.get("status") == "violation" and any(r["outcome"] == "violation" for r in results):
        messages = []
        for result in results:
            if result["outcome"] != "violation":
                continue
            parts = [result["reason"] or ""]
            parts += [bytes(result[stream]).decode("utf-8", errors="replace") for stream in ("stdout", "stderr")]
            detail = "\n".join(part.strip() for part in parts if part.strip()) or "check failed"
            messages.append(f'{result["id"]}: {detail[:1500]}')
        return "violation", "\n".join(messages)[:8000]
    return "incomplete", "IronLint exit code and verdict do not agree"


def expected_ids(binary, config, root, deadline):
    code, stdout, stderr = run([binary, "explain", ".ironlint.yml", "--config", str(config),
                                "--root", str(root), "--format", "json"], root, deadline)
    if code != 0:
        raise RuntimeError("cannot inspect required checks: " + stderr.decode("utf-8", errors="replace"))
    rows = json.loads(stdout.decode("utf-8"))
    if not isinstance(rows, list) or not rows:
        raise RuntimeError("invalid required check list")
    ids = []
    for row in rows:
        if not isinstance(row, dict) or row.get("acceptance") != "required" or not isinstance(row.get("check"), str) or not row["check"]:
            raise RuntimeError("invalid required check list")
        ids.append(row["check"])
    if len(set(ids)) != len(ids):
        raise RuntimeError("duplicate required checks")
    return ids


def feedback(event, kind, diagnostics, command, continued=False):
    if kind == "pass":
        return {}
    text = f"IronLint {kind}: {diagnostics}\nReproduce: {shlex.join(command)}"
    if event == "change":
        return {"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": text}}
    if kind == "violation" and not continued:
        return {"decision": "block", "reason": text + "\nRepair the candidate, then try finishing again. Keep the policy and its checks intact."}
    return {"systemMessage": text + "\nAcceptance remains incomplete. Automatic continuation stopped; resolve this before treating the work as ready."}


def evaluate(payload, mode, timeout=600):
    event = "accept" if mode == "stop" else "change"
    command = [os.environ.get("IRONLINT_BIN", "ironlint"), "check", "--event", event, "--format", "json"]
    continued = payload.get("stop_hook_active") is True
    try:
        expected_event = "Stop" if mode == "stop" else "PostToolUse"
        if payload.get("hook_event_name") != expected_event:
            raise RuntimeError("unexpected hook event")
        cwd = payload.get("cwd")
        if not isinstance(cwd, str) or not Path(cwd).is_absolute():
            raise RuntimeError("hook cwd must be an absolute directory")
        root = Path(cwd).resolve(strict=True)
        if not root.is_dir():
            raise RuntimeError("hook cwd is not a directory")
        config = root / ".ironlint.yml"
        if not config.is_file():
            return feedback(event, "incomplete", "no .ironlint.yml policy in the hook cwd", command, continued) if mode == "stop" else {}
        paths = changed_paths(payload, root) if event == "change" else None
        if paths == []:
            return {}
        command += ["--config", str(config), "--root", str(root)]
        if paths is not None:
            command += [f"--file={path}" for path in paths]
        deadline = time.monotonic() + timeout
        identity = hashlib.sha256(config.read_bytes()).digest()
        expected = expected_ids(command[0], config, root, deadline) if event == "accept" else None
        if event == "accept" and (payload.get("background_tasks") or payload.get("session_crons")):
            raise RuntimeError("background tasks or scheduled writes may still be active")
        code, stdout, stderr = run(command + ["--cancel-on-stdin-close"], root, deadline)
        kind, diagnostics = classify(stdout, code, event, expected)
        if hashlib.sha256(config.read_bytes()).digest() != identity:
            kind, diagnostics = "incomplete", "policy changed during evaluation"
        if kind == "incomplete" and stderr:
            diagnostics += "\n" + stderr.decode("utf-8", errors="replace")[:2000]
        return feedback(event, kind, diagnostics, command, continued)
    except (OSError, RuntimeError, ValueError) as error:
        return feedback(event, "incomplete", str(error)[:4000], command, continued)


def interrupted(_signal, _frame):
    raise RuntimeError("IronLint hook interrupted")


def main():
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    try:
        if len(sys.argv) != 3 or sys.argv[1] not in {"codex", "claude-code"} or sys.argv[2] not in {"post-tool-use", "stop"}:
            raise ValueError("expected harness and post-tool-use or stop")
        raw = sys.stdin.buffer.read(8 * 1024 * 1024 + 1)
        if len(raw) > 8 * 1024 * 1024:
            raise ValueError("hook input exceeded 8 MiB")
        payload = json.loads(raw.decode("utf-8"))
        if not isinstance(payload, dict):
            raise ValueError("hook input must be an object")
        output = evaluate(payload, sys.argv[2])
    except (ValueError, UnicodeError, RuntimeError) as error:
        mode = sys.argv[2] if len(sys.argv) > 2 else "stop"
        output = feedback("change" if mode == "post-tool-use" else "accept", "incomplete", str(error), ["ironlint", "check", "--event", "accept"])
    print(json.dumps(output))


if __name__ == "__main__":
    main()
