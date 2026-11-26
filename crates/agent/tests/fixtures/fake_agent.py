import json
import sys

# A tiny ACP agent over stdio, one JSON message per line, for the hub's tests.

state = {"model": "opus", "effort": "high"}
images = "--images" in sys.argv
sessions = 0


def send(msg):
    msg["jsonrpc"] = "2.0"
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def read():
    line = sys.stdin.readline()
    return json.loads(line) if line else None


def config():
    return [
        {
            "id": "model",
            "name": "Model",
            "category": "model",
            "type": "select",
            "currentValue": state["model"],
            "options": [{"value": "opus", "name": "Opus"}, {"value": "sonnet", "name": "Sonnet"}],
        },
        {
            "id": "effort",
            "name": "Effort",
            "category": "thought_level",
            "type": "select",
            "currentValue": state["effort"],
            "options": [{"value": "low", "name": "low"}, {"value": "high", "name": "high"}],
        },
    ]


MODES = {
    "currentModeId": "default",
    "availableModes": [
        {"id": "default", "name": "Default"},
        {"id": "bypassPermissions", "name": "Bypass Permissions"},
    ],
}


def update(session, body):
    send({"method": "session/update", "params": {"sessionId": session, "update": body}})


def prompt(id, params):
    session = params["sessionId"]
    blocks = params["prompt"]
    text = " ".join(block["text"] for block in blocks if block["type"] == "text")
    for block in blocks:
        if block["type"] == "image":
            text += " [%s %d]" % (block["mimeType"], len(block["data"]))
    update(session, {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "echo: " + text}})
    if "ask" in text:
        update(session, {
            "sessionUpdate": "tool_call",
            "toolCallId": "t1",
            "title": "Edit a.rs",
            "kind": "edit",
            "status": "pending",
            "locations": [{"path": "/p/a.rs"}],
            "content": [{"type": "diff", "path": "/p/a.rs", "oldText": "a\n", "newText": "b\n"}],
        })
        send({
            "id": "perm-1",
            "method": "session/request_permission",
            "params": {
                "sessionId": session,
                "toolCall": {"toolCallId": "t1"},
                "options": [
                    {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                    {"optionId": "no", "name": "Reject", "kind": "reject_once"},
                ],
            },
        })
        while True:
            answer = read()
            if answer is None:
                return
            if answer.get("id") == "perm-1":
                break
        outcome = answer["result"]["outcome"]
        done = outcome.get("outcome") == "selected" and outcome.get("optionId") == "yes"
        update(session, {"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed" if done else "failed"})
    if "run" in text:
        update(session, {
            "sessionUpdate": "tool_call",
            "toolCallId": "t2",
            "title": "`cargo test`",
            "kind": "execute",
            "status": "completed",
            "rawInput": {"command": "cargo test"},
            "content": [{"type": "content", "content": {"type": "text", "text": "test result: ok"}}],
        })
    send({"id": id, "result": {"stopReason": "end_turn"}})


while True:
    msg = read()
    if msg is None:
        break
    method = msg.get("method")
    id = msg.get("id")
    params = msg.get("params") or {}
    if method == "initialize":
        send({"id": id, "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True, "promptCapabilities": {"image": images}}}})
    elif method == "session/new":
        sessions += 1
        send({"id": id, "result": {"sessionId": "s%d" % sessions, "configOptions": config(), "modes": MODES}})
    elif method == "session/load":
        if params["sessionId"] == "lost":
            send({"id": id, "error": {"code": -32000, "message": "no such session"}})
        else:
            update(params["sessionId"], {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "old"}})
            send({"id": id, "result": {"configOptions": config()}})
    elif method == "session/set_config_option":
        state[params["configId"]] = params["value"]
        send({"id": id, "result": {"configOptions": config()}})
    elif method == "session/set_mode":
        send({"id": id, "result": {}})
    elif method == "session/prompt":
        prompt(id, params)
    elif id is not None and method is not None:
        send({"id": id, "error": {"code": -32601, "message": "unknown"}})
