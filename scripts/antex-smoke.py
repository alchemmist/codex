import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading


def response(items, number):
    events = [{"type": "response.created", "response": {"id": f"response-{number}"}}]
    events.extend({"type": "response.output_item.done", "item": item} for item in items)
    events.append(
        {
            "type": "response.completed",
            "response": {
                "id": f"response-{number}",
                "usage": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0},
            },
        }
    )
    return "".join(
        f"event: {event['type']}\ndata: {json.dumps(event)}\n\n" for event in events
    ).encode()


def smoke(binary, scenario):
    requests = []
    assistant = {
        "type": "message",
        "role": "assistant",
        "id": "message-baseline",
        "content": [{"type": "output_text", "text": "antex baseline complete"}],
    }

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            if self.path != "/v1/responses":
                self.send_error(404)
                return
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            requests.append(body)
            if scenario == "shell" and len(requests) == 1:
                items = [
                    {
                        "type": "function_call",
                        "call_id": "baseline-shell",
                        "name": "exec_command",
                        "arguments": json.dumps(
                            {
                                "cmd": "printf antex-shell-marker",
                                "max_output_tokens": 1000,
                            }
                        ),
                    }
                ]
            else:
                items = [assistant]
            payload = response(items, len(requests))
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, format, *args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(prefix="antex-baseline-") as directory:
            root = Path(directory)
            home = root / "home"
            home.mkdir()
            workspace = root / "workspace"
            workspace.mkdir()
            env = {
                key: value
                for key, value in os.environ.items()
                if not key.startswith(("CODEX_", "OPENAI_", "ANTEX_", "CHATGPT_"))
            }
            env.update(
                {
                    "CODEX_HOME": str(home),
                    "CODEX_SQLITE_HOME": str(home),
                    "CODEX_API_KEY": "baseline-dummy",
                    "NO_PROXY": "127.0.0.1,localhost",
                }
            )
            base = f"http://127.0.0.1:{server.server_port}/v1"
            command = [
                str(binary),
                "exec",
                "--skip-git-repo-check",
                "--json",
                "--sandbox",
                "workspace-write",
                "-C",
                str(workspace),
                "-c",
                f"openai_base_url={json.dumps(base)}",
                "-c",
                'model="gpt-5.2-codex"',
                "-c",
                "features.code_mode=false",
            ]
            turn_args = ["Print the baseline marker."]
            if scenario == "image":
                fixture = (
                    Path(__file__).resolve().parents[1]
                    / "codex-rs/skills/src/assets/samples/imagegen/assets/imagegen.png"
                )
                turn_args = ["--image", str(fixture), "--", *turn_args]
            result = subprocess.run(
                [*command, *turn_args],
                env=env,
                cwd=workspace,
                text=True,
                capture_output=True,
                timeout=90,
            )
            if result.returncode:
                raise RuntimeError(
                    f"legacy binary exited {result.returncode}: {result.stderr[-4000:]}"
                )
            if "antex baseline complete" not in result.stdout:
                raise AssertionError(
                    f"assistant response missing: {result.stdout[-2000:]}"
                )
            if scenario == "resume":
                resumed = subprocess.run(
                    [*command, "resume", "--last", "Continue the baseline."],
                    env=env,
                    cwd=workspace,
                    text=True,
                    capture_output=True,
                    timeout=90,
                )
                if (
                    resumed.returncode
                    or "antex baseline complete" not in resumed.stdout
                ):
                    raise AssertionError(f"resume failed: {resumed.stderr[-2000:]}")
                if "antex baseline complete" not in json.dumps(requests[-1]["input"]):
                    raise AssertionError(
                        "resumed model input lost the previous assistant message"
                    )
            expected = 2 if scenario in ["shell", "resume"] else 1
            if len(requests) != expected:
                raise AssertionError(
                    f"expected {expected} requests, received {len(requests)}"
                )
            if scenario == "image":
                images = [
                    content
                    for item in requests[0]["input"]
                    for content in item.get("content", [])
                    if isinstance(content, dict)
                    and content.get("type") == "input_image"
                ]
                if len(images) != 1 or not images[0]["image_url"].startswith(
                    "data:image/"
                ):
                    raise AssertionError("image missing from model request")
            if scenario == "shell":
                outputs = [
                    item
                    for item in requests[1]["input"]
                    if item.get("type") == "function_call_output"
                    and item.get("call_id") == "baseline-shell"
                ]
                if len(outputs) != 1 or "antex-shell-marker" not in json.dumps(
                    outputs[0]
                ):
                    raise AssertionError(f"shell result missing: {outputs}")
            sessions = list(home.glob("sessions/**/*.jsonl"))
            if len(sessions) != 1:
                raise AssertionError(
                    f"expected one persisted session, received {len(sessions)}"
                )
            records = [
                json.loads(line) for line in sessions[0].read_text().splitlines()
            ]
            if not any(
                "antex baseline complete" in json.dumps(record) for record in records
            ):
                raise AssertionError(
                    "assistant response missing from persisted session"
                )
            return {
                "scenario": scenario,
                "requests": len(requests),
                "session_records": len(records),
                "status": "passed",
            }
    finally:
        server.shutdown()
        server.server_close()
        worker.join()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument(
        "--scenario", choices=["assistant", "shell", "resume", "image"], action="append"
    )
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    for scenario in args.scenario or ["assistant", "shell", "resume", "image"]:
        print(json.dumps(smoke(binary, scenario)), flush=True)


if __name__ == "__main__":
    main()
