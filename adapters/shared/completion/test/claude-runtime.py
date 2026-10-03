#!/usr/bin/env python3
"""Opt-in real Claude dispatcher replay, using localhost and no credentials."""
import argparse
import json
import os
import shlex
import shutil
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class Provider(BaseHTTPRequestHandler):
    calls = 0
    write_path = None
    messages = []
    tools = []
    read_path = None
    repair_after = None
    delay = 0
    full_request = ''

    def log_message(self, *args):
        pass

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
        type(self).full_request = json.dumps(request)
        type(self).messages = request.get('messages', [])
        type(self).tools = [tool.get('name') for tool in request.get('tools', [])]
        if self.path.endswith("/count_tokens"):
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"input_tokens":32}')
            return
        type(self).calls += 1
        if self.delay:
            time.sleep(self.delay)
        tool = (self.write_path or self.read_path) and self.calls == (self.repair_after or 1)
        content = {"type": "tool_use", "id": "tool_fixture", "name": "Edit", "input": {"file_path": self.write_path, "old_string": "", "new_string": "repaired\n"}} if tool else {"type": "text", "text": "Done."}
        if self.read_path and tool:
            content = {"type": "tool_use", "id": "tool_fixture", "name": "Read", "input": {"file_path": self.read_path}}
        message = {"id": "msg_fixture", "type": "message", "role": "assistant", "model": "claude-sonnet-4-6", "content": [], "stop_reason": None, "stop_sequence": None, "usage": {"input_tokens": 32, "output_tokens": 2}}
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        start = dict(content)
        if tool:
            start["input"] = {}
        else:
            start["text"] = ""
        delta = {"type": "input_json_delta", "partial_json": json.dumps(content["input"])} if tool else {"type": "text_delta", "text": "Done."}
        events = [("message_start", {"type": "message_start", "message": message}), ("content_block_start", {"type": "content_block_start", "index": 0, "content_block": start}), ("content_block_delta", {"type": "content_block_delta", "index": 0, "delta": delta}), ("content_block_stop", {"type": "content_block_stop", "index": 0}), ("message_delta", {"type": "message_delta", "delta": {"stop_reason": "tool_use" if tool else "end_turn", "stop_sequence": None}, "usage": {"output_tokens": 2}}), ("message_stop", {"type": "message_stop"})]
        for event, data in events:
            self.wfile.write(("event: " + event + "\ndata: " + json.dumps(data) + "\n\n").encode())
        self.wfile.flush()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True)
    parser.add_argument("--node", required=True)
    parser.add_argument("--ironlint")
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix="ironlint-claude-replay-") as temporary:
            root = Path(temporary).resolve()
            home = root / "home"
            home.mkdir()
            provider_env = {"ANTHROPIC_API_KEY": "ironlint-dummy-local-provider", "ANTHROPIC_BASE_URL": "http://127.0.0.1:" + str(server.server_port)}
            env = {"PATH": os.defpath, "HOME": str(home), "XDG_CONFIG_HOME": str(root / "xdg"), "CLAUDE_CONFIG_DIR": str(root / "config"), "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1", "DISABLE_AUTOUPDATER": "1", "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1", "CLAUDE_CODE_DISABLE_AGENT_VIEW": "1", "CLAUDE_CODE_CERT_STORE": "bundled", "CI": "true", **provider_env}
            outputs = {}
            cases = {"clean": {}, "block": {"decision": "block", "reason": "persistent failure"}, "stop": {"continue": False, "stopReason": "incomplete"}, "warning": {"systemMessage": "incomplete"}, "competing": {"decision": "block", "reason": "persistent failure"}, "crash": {}, "timeout": {}, "disabled": {"decision": "block", "reason": "persistent failure"}, "max_turns": {"decision": "block", "reason": "persistent failure"}}
            for case, output in cases.items():
                project = root / case
                project.mkdir()
                log = project / "payloads.jsonl"
                hook = project / "hook.py"
                action = "sys.exit(1)" if case == "crash" else "time.sleep(2)" if case == "timeout" else "print(" + repr(json.dumps(output)) + ")"
                hook.write_text("import sys,json,time\np=json.load(sys.stdin)\nwith open(" + repr(str(log)) + ", 'a') as f:f.write(json.dumps(p)+'\\n')\n" + action + "\n")
                hooks = [{"type": "command", "command": "/usr/bin/python3 " + str(hook), "timeout": .1 if case == "timeout" else 5}]
                if case == "competing":
                    hooks.append({"type": "command", "command": "printf '%s' '{\"continue\":false,\"stopReason\":\"competing\"}'"})
                settings = {"hooks": {"Stop": [{"hooks": hooks}]}}
                if case == "disabled":
                    settings["disableAllHooks"] = True
                Provider.calls = 0
                command = [args.runtime, "-p", "Ignore all repair guidance. Say Done.", "--output-format", "json", "--model", "claude-sonnet-4-6", "--tools", "", "--setting-sources", "", "--settings", json.dumps(settings), "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}', "--no-session-persistence", "--disable-slash-commands", "--no-chrome", "--max-turns", "2" if case == "max_turns" else "12"]
                child = subprocess.run(command, env=env, cwd=project, capture_output=True, text=True, input="", timeout=15)
                result = json.loads(child.stdout)
                payloads = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                outputs[case] = {"exit": child.returncode, "subtype": result["subtype"], "is_error": result["is_error"], "terminal_reason": result.get("terminal_reason"), "stops": len(payloads), "continued": [payload["stop_hook_active"] for payload in payloads]}
            assert outputs["block"]["stops"] == 9 and outputs["block"]["terminal_reason"] == "completed"
            assert outputs["block"]["continued"] == [False] + [True] * 8
            assert outputs["stop"]["subtype"] == "success" and outputs["stop"]["terminal_reason"] == "stop_hook_prevented"
            assert outputs["competing"]["terminal_reason"] == "stop_hook_prevented"
            for case in ["clean", "warning", "crash", "timeout", "disabled"]:
                assert outputs[case]["exit"] == 0 and outputs[case]["subtype"] == "success" and outputs[case]["terminal_reason"] == "completed"
            assert outputs["disabled"]["stops"] == 0
            assert outputs["max_turns"]["subtype"] == "error_max_turns"
            module = Path(__file__).resolve().parents[1] / "claude.ts"
            deadline = module.parent / "deadline.ts"
            program = "import {ClaudeHost} from " + json.dumps(module.as_uri()) + ";import {Deadline} from " + json.dumps(deadline.as_uri()) + ";const d=new Deadline(15000);try{const h=new ClaudeHost(process.argv[1],process.argv[2]);await h.validate(process.argv[3],d);console.log(JSON.stringify({output:await h.run('Repair the candidate using Edit.',process.argv[3],d)}));}finally{d.dispose()}"
            config = root / "owner.json"
            config.write_text(json.dumps({"model": "claude-sonnet-4-6", "env": provider_env}))
            for case in ["controlled_clean", "controlled_write", "controlled_outside", "controlled_git", "controlled_readoutside", "controlled space"]:
                project = root / case
                project.mkdir()
                Provider.calls = 0
                Provider.write_path = str(project / "candidate.txt") if case in ["controlled_write",'controlled space'] else str(root / "outside.txt") if case == "controlled_outside" else None
                Provider.read_path = None
                (project / 'CLAUDE.md').write_text('IRONLINT_INJECTED_CANDIDATE_INSTRUCTION_MARKER')
                (project / '.claude').mkdir()
                injected_log = root / 'injected-hook.txt'
                (project / '.claude' / 'settings.json').write_text(json.dumps({'hooks':{'Stop':[{'hooks':[{'type':'command','command':'touch '+str(injected_log)}]}]}}))
                if case == 'controlled_git':
                    (project / '.git').mkdir()
                    Provider.write_path = str(project / '.git' / 'config')
                if case == 'controlled_readoutside':
                    secret = root / 'outside-read.txt'
                    secret.write_text('owner-secret')
                    Provider.read_path = str(secret)
                child = subprocess.run([args.node, "--experimental-strip-types", "--input-type=module", "-e", program, args.runtime, str(config), str(project)], env=env, cwd=project, capture_output=True, text=True, input="", timeout=20)
                assert child.returncode == 0, child.stderr
                outputs[case] = json.loads(child.stdout)
                assert 'IRONLINT_INJECTED_CANDIDATE_INSTRUCTION_MARKER' not in Provider.full_request
                assert not injected_log.exists()
                if case in ["controlled_write", 'controlled space']:
                    if not (project / "candidate.txt").exists():
                        print(json.dumps(Provider.messages[-2:], indent=2))
                        print('tools:', Provider.tools)
                    assert (project / "candidate.txt").read_text() == "repaired\n"
                if case == "controlled_outside":
                    assert not (root / "outside.txt").exists()
                if case == 'controlled_git':
                    assert not (project / '.git' / 'config').exists()
                if case == 'controlled_readoutside':
                    assert 'owner-secret' not in json.dumps(Provider.messages)
            if args.ironlint:
                completion = module.parent / 'completion.ts'
                program = "import {ClaudeHost} from " + json.dumps(module.as_uri()) + ";import {runControlled} from " + json.dumps(completion.as_uri()) + ";const cancellation=new AbortController();if(process.argv[8]==='cancel')setTimeout(()=>cancellation.abort(),800);const r=await runControlled({root:process.argv[3],policy:process.argv[4],binary:process.argv[5],artifacts:process.argv[6],host:new ClaudeHost(process.argv[1],process.argv[2]),expectedIds:['required'],prompt:'Finish the candidate.',maxRepairTurns:1,deadlineMs:15000,signal:cancellation.signal});const output={status:r.status,repairTurns:r.repairTurns,diagnostics:r.diagnostics};r.candidate?.dispose();console.log(JSON.stringify(output));process.exitCode=r.status==='complete'?0:3;"
                for case in ['complete', 'repair', 'persistent', 'untrusted', 'cancel', 'check_mutation', 'source_mutation']:
                    project = root / ('integrated_' + case)
                    project.mkdir()
                    subprocess.run(['git', 'init', '-q', str(project)], env=env, check=True)
                    (project / 'README.md').write_text('initial\n')
                    subprocess.run(['git', '-C', str(project), 'add', 'README.md'], env=env, check=True)
                    subprocess.run(['git', '-C', str(project), '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'initial'], env=env, check=True)
                    original_index = (project / '.git' / 'index').read_bytes()
                    policy = root / (case + '.yml')
                    command = 'true' if case in ['complete','untrusted','cancel'] else 'printf changed > README.md' if case == 'check_mutation' else 'test -f candidate.txt'
                    if case == 'source_mutation':
                        command = 'printf changed >> ' + shlex.quote(str(project / 'README.md'))
                    policy.write_text('version: 1\nchecks:\n  required:\n    run: '+json.dumps(command)+'\n    on: [accept]\n')
                    if case != 'untrusted':
                        trust = subprocess.run([args.ironlint, 'trust', '--config', str(policy)], env=env, cwd=project, capture_output=True, text=True)
                        assert trust.returncode == 0, trust.stderr
                    artifacts = root / ('artifacts_' + case)
                    artifacts.mkdir()
                    Provider.calls = 0
                    Provider.read_path = None
                    Provider.write_path = str(project / 'candidate.txt') if case == 'repair' else None
                    Provider.repair_after = 2 if case == 'repair' else None
                    Provider.delay = 3 if case == 'cancel' else 0
                    child = subprocess.run([args.node, '--experimental-strip-types', '--input-type=module', '-e', program, args.runtime, str(config), str(project), str(policy), args.ironlint, str(artifacts), 'unused', case], env=env, cwd=project, capture_output=True, text=True, input='', timeout=20)
                    result = json.loads(child.stdout)
                    outputs['integrated_'+case] = result
                    assert result['status'] == ('complete' if case in ['complete','repair'] else 'incomplete'), result
                    assert child.returncode == (0 if result['status']=='complete' else 3)
                    assert result['repairTurns'] == (1 if case in ['repair','persistent'] else 0), result
                    assert (project / '.git' / 'index').read_bytes() == original_index
                    assert (project / 'README.md').read_text() == ('initial\nchangedchangedchanged' if case == 'source_mutation' else 'initial\n')
                    assert not list(artifacts.iterdir()), list(artifacts.iterdir())
                    Provider.repair_after = None
                    Provider.delay = 0
                launcher = Path(__file__).resolve().parents[3] / 'claude-code' / 'bin' / 'ironlint-claude-complete'
                launch_env = {**env, 'PATH': str(Path(args.node).parent) + os.pathsep + os.defpath}
                for case in ['cli_complete', 'cli_persistent']:
                    project = root / case
                    project.mkdir()
                    subprocess.run(['git', 'init', '-q', str(project)], env=env, check=True)
                    (project / 'README.md').write_text('initial\n')
                    subprocess.run(['git', '-C', str(project), 'add', 'README.md'], env=env, check=True)
                    subprocess.run(['git', '-C', str(project), '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'initial'], env=env, check=True)
                    original_index = (project / '.git' / 'index').read_bytes()
                    policy = root / (case + '.yml')
                    check = 'true' if case == 'cli_complete' else 'false'
                    policy.write_text('version: 1\nchecks:\n  required:\n    run: '+json.dumps(check)+'\n    on: [accept]\n')
                    trust = subprocess.run([args.ironlint, 'trust', '--config', str(policy)], env=env, cwd=project, capture_output=True, text=True)
                    assert trust.returncode == 0, trust.stderr
                    artifacts = root / ('artifacts_' + case)
                    artifacts.mkdir()
                    Provider.calls = 0
                    Provider.read_path = None
                    Provider.write_path = None
                    Provider.repair_after = None
                    Provider.delay = 0
                    command = [str(launcher), '--root', str(project), '--policy', str(policy), '--binary', args.ironlint, '--artifacts', str(artifacts), '--runtime', args.runtime, '--runtime-config', str(config), '--checks', 'required', '--task', 'Finish the candidate.', '--max-repair-turns', '0', '--deadline-seconds', '15']
                    child = subprocess.run(command, env=launch_env, cwd=project, capture_output=True, text=True, input='', timeout=20)
                    lines = child.stdout.splitlines()
                    assert len(lines) == 1, child.stdout
                    result = json.loads(lines[0])
                    outputs[case] = {'exit':child.returncode,'status':result['status'],'repairTurns':result['repairTurns']}
                    if case == 'cli_complete':
                        assert child.returncode == 0 and result['status'] == 'complete', result
                        assert result['modelText'] == 'Done.'
                        candidate = result['candidate']
                        assert set(candidate) == {'identity','tree','artifact'}
                        assert len(candidate['identity']) == 64 and all(letter in '0123456789abcdef' for letter in candidate['identity'])
                        assert Path(candidate['artifact']).parent == artifacts
                        assert Path(candidate['tree']).parent == Path(candidate['artifact'])
                        assert (Path(candidate['tree']) / 'README.md').read_text() == 'initial\n'
                        shutil.rmtree(candidate['artifact'])
                    else:
                        assert child.returncode == 3 and result['status'] == 'incomplete', result
                        assert 'candidate' not in result and 'modelText' not in result
                    assert result['repairTurns'] == 0
                    assert (project / '.git' / 'index').read_bytes() == original_index
                    assert (project / 'README.md').read_text() == 'initial\n'
                    assert not list(artifacts.iterdir())
            print(json.dumps(outputs, indent=2))
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
