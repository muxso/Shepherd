import hmac
import json
import os
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

TOKEN = os.environ["SHEPHERD_SCRIPT_RUNNER_TOKEN"]
if len(TOKEN) < 32:
    raise RuntimeError("Invalid runner token")
SLOTS = threading.BoundedSemaphore(2)
LIMIT = 32768
COMMANDS = {
    "python": ["/usr/bin/python3", "-I", "-B", "/script.py"],
    "javascript": ["/usr/bin/node", "--jitless", "--no-expose-wasm", "--max-old-space-size=96", "/script.cjs"],
}


def run(language, source, payload):
    started = time.monotonic()
    command = COMMANDS[language]
    with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        path = Path(directory) / "script"
        path.write_text(source, encoding="utf-8")
        sandbox = [
            "/usr/bin/prlimit", "--as=2147483648", "--cpu=4", "--fsize=32768", "--nofile=64",
            "--core=0", "--", "/usr/bin/bwrap", "--unshare-all", "--unshare-user", "--disable-userns",
            "--new-session", "--die-with-parent", "--cap-drop", "ALL", "--clearenv",
            "--ro-bind", "/usr/lib", "/usr/lib", "--ro-bind-try", "/lib", "/lib",
            "--ro-bind-try", "/lib64", "/lib64", "--ro-bind-try", "/usr/share/nodejs", "/usr/share/nodejs",
            "--ro-bind", command[0], command[0], "--ro-bind", str(path), command[-1],
            "--dev", "/dev", "--size", "1048576", "--tmpfs", "/tmp", "--chdir", "/tmp",
            "--setenv", "HOME", "/tmp", "--setenv", "TZ", "UTC", "--setenv", "LANG", "C.UTF-8",
            "--", *command,
        ]
        with subprocess.Popen(sandbox, stdin=subprocess.PIPE, stdout=stdout, stderr=stderr,
                              env={}, close_fds=True) as process:
            timed_out = False
            try:
                process.communicate(payload, timeout=6)
            except subprocess.TimeoutExpired:
                process.kill()
                process.communicate()
                timed_out = True
        limited = any(os.fstat(file.fileno()).st_size >= LIMIT for file in (stdout, stderr))
        def read(file):
            file.seek(0)
            return file.read(LIMIT).decode("utf-8", errors="replace")
        return {"stdout": read(stdout), "stderr": read(stderr),
                "exitCode": None if timed_out else process.returncode, "timedOut": timed_out,
                "durationMs": round((time.monotonic() - started) * 1000), "outputLimited": limited}


class Handler(BaseHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(8)

    def reply(self, status, body):
        content = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(content)))
        self.end_headers()
        self.wfile.write(content)

    def do_POST(self):
        if self.path != "/run":
            self.reply(404, {})
            return
        if not hmac.compare_digest(self.headers.get("Authorization", ""), "Bearer " + TOKEN):
            self.reply(401, {})
            return
        try:
            size = int(self.headers.get("Content-Length", "0"))
            if not 0 < size <= 262144:
                raise ValueError()
            data = json.loads(self.rfile.read(size))
            language, source = data["language"], data["source"]
            payload = json.dumps(data["input"], ensure_ascii=False, allow_nan=False).encode()
            if language not in COMMANDS or not 0 < len(source.encode()) <= 65536 or len(payload) > 65536:
                raise ValueError()
        except (ValueError, TypeError, KeyError, AttributeError):
            self.reply(400, {})
            return
        if not SLOTS.acquire(blocking=False):
            self.reply(429, {})
            return
        try:
            self.reply(200, run(language, source, payload))
        except OSError:
            self.reply(503, {})
        finally:
            SLOTS.release()


if __name__ == "__main__":
    ThreadingHTTPServer((os.environ.get("SHEPHERD_SCRIPT_RUNNER_BIND", "127.0.0.1"), 18087), Handler).serve_forever()
