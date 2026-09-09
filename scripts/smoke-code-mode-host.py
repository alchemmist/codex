import argparse
import json
import queue
import struct
import subprocess
import tempfile
import threading

MAX_FRAME_BYTES = 64 * 1024 * 1024
TIMEOUT_SECONDS = 15


def smoke_host(binary):
    messages = queue.Queue(maxsize=32)
    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(
            [str(binary), "--listen", "stdio"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=errors,
        )

        def read_frames():
            try:
                while True:
                    header = process.stdout.read(4)
                    if len(header) != 4:
                        raise RuntimeError("code-mode host closed its stdout")
                    size = struct.unpack("<I", header)[0]
                    if size > MAX_FRAME_BYTES:
                        raise RuntimeError("oversized code-mode frame")
                    payload = process.stdout.read(size)
                    if len(payload) != size:
                        raise RuntimeError("incomplete code-mode frame")
                    messages.put(json.loads(payload))
            except (OSError, ValueError, RuntimeError) as error:
                messages.put(error)

        reader = threading.Thread(target=read_frames, daemon=True)
        reader.start()

        def send(message):
            payload = json.dumps(message).encode()
            process.stdin.write(struct.pack("<I", len(payload)) + payload)
            process.stdin.flush()

        def receive():
            message = messages.get(timeout=TIMEOUT_SECONDS)
            if isinstance(message, Exception):
                raise message
            return message

        try:
            send(
                {
                    "type": "connection/hello",
                    "supportedVersions": [1],
                    "requiredCapabilities": [],
                    "optionalCapabilities": [],
                }
            )
            assert receive()["type"] == "connection/ready"
            send(
                {
                    "type": "operation/request",
                    "id": 1,
                    "request": {"method": "session/open", "sessionId": "smoke"},
                }
            )
            assert receive()["result"]["value"]["type"] == "session/ready"
            tool = {
                "name": "smoke_echo",
                "tool_name": {"name": "smoke_echo", "namespace": None},
                "description": "Return a smoke-test value",
                "kind": "function",
                "input_schema": {
                    "type": "object",
                    "properties": {"value": {"type": "integer"}},
                    "required": ["value"],
                    "additionalProperties": False,
                },
                "output_schema": None,
            }
            send(
                {
                    "type": "operation/request",
                    "id": 2,
                    "request": {
                        "method": "session/execute",
                        "sessionId": "smoke",
                        "request": {
                            "tool_call_id": "smoke-call",
                            "enabled_tools": [tool],
                            "source": "const r = await tools.smoke_echo({value: 40}); text(r.value + 2);",
                            "yield_time_ms": 10000,
                            "max_output_tokens": 100,
                        },
                    },
                }
            )
            delegated = False
            for _ in range(16):
                message = receive()
                if message["type"] == "delegate/request":
                    invocation = message["request"]["invocation"]
                    assert invocation["tool_name"] == tool["tool_name"]
                    assert invocation["input"] == {"value": 40}
                    delegated = True
                    send(
                        {
                            "type": "delegate/response",
                            "id": message["id"],
                            "result": {
                                "status": "ok",
                                "value": {
                                    "type": "tool/result",
                                    "result": {"value": 40},
                                },
                            },
                        }
                    )
                elif message["type"] == "execute/initialResponse":
                    result = message["result"]["value"]["Result"]
                    assert result["error_text"] is None, result
                    assert result["content_items"] == [
                        {"type": "input_text", "text": "42"}
                    ], result
                    assert delegated, "nested tool was not invoked"
                    break
            else:
                raise RuntimeError("execution did not complete")
            send(
                {
                    "type": "operation/request",
                    "id": 3,
                    "request": {"method": "session/shutdown", "sessionId": "smoke"},
                }
            )
            for _ in range(16):
                message = receive()
                if message.get("id") == 3:
                    assert message["result"]["status"] == "ok"
                    break
            else:
                raise RuntimeError("session did not shut down")
            process.stdin.close()
            assert process.wait(timeout=TIMEOUT_SECONDS) == 0
        except (
            AssertionError,
            OSError,
            ValueError,
            RuntimeError,
            queue.Empty,
            subprocess.TimeoutExpired,
        ):
            errors.seek(0)
            print(errors.read()[-4000:].decode(errors="replace"))
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            reader.join(timeout=5)
    print(
        "Code-mode host passed: JavaScript, nested tool invocation, result, clean shutdown"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("binary")
    smoke_host(parser.parse_args().binary)
