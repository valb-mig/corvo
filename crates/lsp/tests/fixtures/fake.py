import json
import sys

ready = False


def read():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.decode().strip()
        if not line:
            break
        key, value = line.split(":", 1)
        headers[key.lower()] = value.strip()
    return json.loads(sys.stdin.buffer.read(int(headers["content-length"])))


def send(msg):
    msg["jsonrpc"] = "2.0"
    body = json.dumps(msg).encode()
    sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    sys.stdout.buffer.flush()


def diagnose(uri, message):
    span = {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}
    send({
        "method": "textDocument/publishDiagnostics",
        "params": {"uri": uri, "diagnostics": [{"range": span, "message": message}]},
    })


while (msg := read()) is not None:
    method = msg.get("method")
    if method == "initialize":
        send({"id": 999, "method": "workspace/configuration",
              "params": {"items": [{"section": "fake.answer"}]}})
        send({"id": msg["id"], "result": {"capabilities": {
            "textDocumentSync": 2,
            "completionProvider": {"triggerCharacters": ["."]},
        }}})
    elif method == "initialized":
        ready = True
    elif msg.get("id") == 999:
        pass
    elif method == "textDocument/didOpen":
        uri = msg["params"]["textDocument"]["uri"]
        diagnose(uri, "opened" if ready else "early")
    elif method == "textDocument/didChange":
        params = msg["params"]
        change = params["contentChanges"][0]
        kind = "range" if "range" in change else "full"
        diagnose(params["textDocument"]["uri"], f"v{params['textDocument']['version']} {kind} {change['text']}")
    elif method == "textDocument/completion":
        send({"id": msg["id"], "result": [{"label": "hello"}]})
    elif method == "shutdown":
        send({"id": msg["id"], "result": None})
    elif method == "exit":
        break
