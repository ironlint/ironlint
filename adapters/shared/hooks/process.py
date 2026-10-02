"""Bounded, owned evaluator subprocesses for native command hooks (POSIX)."""
import os
import selectors
import signal
import subprocess
import time

STDOUT_LIMIT = 8 * 1024 * 1024
STDERR_LIMIT = 64 * 1024
GRACE_SECONDS = 2


def cleanup(child):
    if child.stdin and not child.stdin.closed:
        child.stdin.close()
    try:
        child.wait(timeout=GRACE_SECONDS)
    except subprocess.TimeoutExpired:
        pass
    # The group may still own pipes after its leader exits.
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait(timeout=GRACE_SECONDS)


def run(argv, root, deadline, stdout_limit=STDOUT_LIMIT):
    if time.monotonic() >= deadline:
        raise RuntimeError("IronLint hook deadline exceeded")
    child = subprocess.Popen(argv, cwd=root, stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             start_new_session=True)
    stdout, stderr = bytearray(), bytearray()
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ, stdout)
            selector.register(child.stderr, selectors.EVENT_READ, stderr)
            while selector.get_map():
                if time.monotonic() >= deadline:
                    raise RuntimeError("IronLint hook deadline exceeded")
                for key, _ in selector.select(min(0.05, max(0, deadline - time.monotonic()))):
                    chunk = key.fileobj.read1(65536)
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    if key.data is stdout:
                        stdout.extend(chunk)
                        if len(stdout) > stdout_limit:
                            raise RuntimeError("IronLint output exceeded the hook limit")
                    else:
                        stderr.extend(chunk[:max(0, STDERR_LIMIT - len(stderr))])
            try:
                child.wait(timeout=max(0.001, deadline - time.monotonic()))
            except subprocess.TimeoutExpired as error:
                raise RuntimeError("IronLint hook deadline exceeded") from error
            return child.returncode, bytes(stdout), bytes(stderr)
    finally:
        cleanup(child)
        child.stdout.close()
        child.stderr.close()
