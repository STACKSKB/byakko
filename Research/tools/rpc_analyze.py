"""Analyze opaque HTTP/1.1 gRPC-Web captures from rpc_capture.

The analyzer is deliberately offline and stdlib-only. It records RPC method
names, transport framing, protobuf wire shapes, and HID request/response fields.
For non-HID methods (including database APIs), field values are redacted so the
analysis JSON does not reproduce application database contents or device paths.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


DEFAULT_CAPTURE_DIR = Path("Research/captures")
DEFAULT_OUTPUT = DEFAULT_CAPTURE_DIR / "rpc-analysis.json"
HID_METHODS = {"sendMsg", "readMsg", "sendRawFeature", "readRawFeature"}
SEND_METHODS = {"sendMsg", "sendRawFeature"}
READ_METHODS = {"readMsg", "readRawFeature"}
MAX_HEX_BYTES = 4096


@dataclass
class HttpMessage:
    start_line: str
    headers: dict[str, str]
    body: bytes
    framing: str
    raw_start: int
    raw_end: int
    complete: bool = True


def parse_headers(block: bytes) -> tuple[str, dict[str, str]]:
    lines = block.split(b"\r\n")
    start_line = lines[0].decode("latin1", errors="replace")
    headers: dict[str, str] = {}
    for line in lines[1:]:
        if b":" not in line:
            continue
        key, value = line.split(b":", 1)
        headers[key.decode("latin1", errors="replace").strip().lower()] = value.decode(
            "latin1", errors="replace"
        ).strip()
    return start_line, headers


def parse_chunked_body(data: bytes, body_start: int) -> tuple[bytes, int, bool]:
    cursor = body_start
    chunks: list[bytes] = []
    while True:
        line_end = data.find(b"\r\n", cursor)
        if line_end < 0:
            return b"".join(chunks), len(data), False
        size_line = data[cursor:line_end].split(b";", 1)[0].strip()
        try:
            size = int(size_line, 16)
        except ValueError:
            return b"".join(chunks), line_end + 2, False
        cursor = line_end + 2
        if size == 0:
            # A zero-sized chunk followed immediately by CRLF has no trailer
            # fields. On a persistent HTTP/1.1 connection the next bytes may
            # already be the next response header, so check this before
            # searching for a later trailer terminator.
            if data[cursor:cursor + 2] == b"\r\n":
                return b"".join(chunks), cursor + 2, True
            trailer_end = data.find(b"\r\n\r\n", cursor)
            if trailer_end < 0:
                return b"".join(chunks), len(data), False
            return b"".join(chunks), trailer_end + 4, True
        chunk_end = cursor + size
        if chunk_end + 2 > len(data):
            return b"".join(chunks), len(data), False
        if data[chunk_end:chunk_end + 2] != b"\r\n":
            return b"".join(chunks), chunk_end, False
        chunks.append(data[cursor:chunk_end])
        cursor = chunk_end + 2


def parse_http_stream(data: bytes) -> tuple[list[HttpMessage], int]:
    messages: list[HttpMessage] = []
    cursor = 0
    while cursor < len(data):
        header_end = data.find(b"\r\n\r\n", cursor)
        if header_end < 0:
            break
        start_line, headers = parse_headers(data[cursor:header_end])
        body_start = header_end + 4
        transfer_encoding = headers.get("transfer-encoding", "").lower()
        if "chunked" in transfer_encoding:
            body, message_end, complete = parse_chunked_body(data, body_start)
            framing = "chunked"
        elif "content-length" in headers:
            try:
                length = int(headers["content-length"])
            except ValueError:
                break
            message_end = body_start + length
            complete = message_end <= len(data)
            body = data[body_start:min(message_end, len(data))]
            framing = "content-length"
        else:
            # The captures under analysis use Content-Length or chunked
            # encoding. Without either, the body boundary is unknowable on a
            # persistent connection, so leave it as trailing bytes.
            break
        if not complete:
            # A server-streaming response can remain open without a terminating
            # zero chunk. Keep complete chunks already present so the ordered
            # exchange still records the available messages.
            if body:
                messages.append(
                    HttpMessage(
                        start_line=start_line,
                        headers=headers,
                        body=body,
                        framing=framing,
                        raw_start=cursor,
                        raw_end=len(data),
                        complete=False,
                    )
                )
            cursor = len(data)
            break
        messages.append(
            HttpMessage(
                start_line=start_line,
                headers=headers,
                body=body,
                framing=framing,
                raw_start=cursor,
                raw_end=message_end,
            )
        )
        cursor = message_end
    return messages, len(data) - cursor


def grpc_frames(body: bytes, content_type: str) -> tuple[list[dict[str, Any]], list[str]]:
    errors: list[str] = []
    encoded = body
    text_mode = "grpc-web-text" in content_type.lower()
    if text_mode:
        compact = b"".join(body.split())
        # Chromium's grpc-web text transport may base64-encode each HTTP
        # chunk independently. A padding character in the middle therefore
        # must begin a new decode segment rather than terminate the whole
        # stream. If there is no padding until the end, this also handles one
        # ordinary base64 body.
        segments: list[bytes] = []
        cursor = 0
        while cursor < len(compact):
            padding = compact.find(b"=", cursor)
            if padding < 0:
                segments.append(compact[cursor:])
                break
            end = padding + 1
            if end < len(compact) and compact[end:end + 1] == b"=":
                end += 1
            segments.append(compact[cursor:end])
            cursor = end
        try:
            decoded_segments = []
            for segment in segments:
                if len(segment) % 4:
                    segment += b"=" * ((4 - len(segment) % 4) % 4)
                decoded_segments.append(base64.b64decode(segment, validate=False))
            encoded = b"".join(decoded_segments)
        except (binascii.Error, ValueError) as error:
            return [], [f"base64 decode failed: {error}"]

    frames: list[dict[str, Any]] = []
    cursor = 0
    while cursor < len(encoded):
        if len(encoded) - cursor < 5:
            errors.append("trailing bytes shorter than a gRPC-Web frame header")
            break
        flags = encoded[cursor]
        length = int.from_bytes(encoded[cursor + 1:cursor + 5], "big")
        payload_start = cursor + 5
        payload_end = payload_start + length
        if payload_end > len(encoded):
            errors.append("truncated gRPC-Web frame payload")
            break
        payload = encoded[payload_start:payload_end]
        trailer = bool(flags & 0x80)
        frame: dict[str, Any] = {
            "flags": flags,
            "kind": "trailers" if trailer else "message",
            "length": length,
        }
        if trailer:
            frame["trailers"] = parse_trailers(payload)
        else:
            frame["payload_summary"] = generic_wire_summary(payload)
            frame["payload"] = payload
        frames.append(frame)
        cursor = payload_end
    return frames, errors


def parse_trailers(payload: bytes) -> dict[str, str]:
    trailers: dict[str, str] = {}
    for line in payload.split(b"\r\n"):
        if b":" not in line:
            continue
        key, value = line.split(b":", 1)
        trailers[key.decode("latin1", errors="replace").strip().lower()] = value.decode(
            "latin1", errors="replace"
        ).strip()
    return trailers


def read_varint(data: bytes, cursor: int) -> tuple[int, int] | None:
    value = 0
    shift = 0
    while cursor < len(data) and shift <= 63:
        byte = data[cursor]
        cursor += 1
        value |= (byte & 0x7F) << shift
        if byte < 0x80:
            return value, cursor
        shift += 7
    return None


def decode_wire_fields(data: bytes) -> tuple[list[tuple[int, int, Any]], list[str]]:
    fields: list[tuple[int, int, Any]] = []
    errors: list[str] = []
    cursor = 0
    while cursor < len(data):
        key_result = read_varint(data, cursor)
        if key_result is None:
            errors.append("invalid protobuf field key")
            break
        key, cursor = key_result
        number, wire_type = key >> 3, key & 7
        if number == 0:
            errors.append("protobuf field number zero")
            break
        if wire_type == 0:
            result = read_varint(data, cursor)
            if result is None:
                errors.append(f"truncated varint field {number}")
                break
            value, cursor = result
        elif wire_type == 1:
            if cursor + 8 > len(data):
                errors.append(f"truncated fixed64 field {number}")
                break
            value, cursor = data[cursor:cursor + 8], cursor + 8
        elif wire_type == 2:
            result = read_varint(data, cursor)
            if result is None:
                errors.append(f"truncated length field {number}")
                break
            length, cursor = result
            end = cursor + length
            if end > len(data):
                errors.append(f"truncated bytes field {number}")
                break
            value, cursor = data[cursor:end], end
        elif wire_type == 5:
            if cursor + 4 > len(data):
                errors.append(f"truncated fixed32 field {number}")
                break
            value, cursor = data[cursor:cursor + 4], cursor + 4
        else:
            errors.append(f"unsupported protobuf wire type {wire_type} in field {number}")
            break
        fields.append((number, wire_type, value))
    return fields, errors


def generic_wire_summary(data: bytes) -> dict[str, Any]:
    fields, errors = decode_wire_fields(data)
    summary: list[dict[str, Any]] = []
    for number, wire_type, value in fields:
        item: dict[str, Any] = {"field": number, "wire_type": wire_type}
        if wire_type == 0:
            item["kind"] = "varint"
        elif wire_type == 1:
            item["kind"] = "fixed64"
            item["length"] = len(value)
        elif wire_type == 2:
            item["kind"] = "bytes"
            item["length"] = len(value)
        elif wire_type == 5:
            item["kind"] = "fixed32"
            item["length"] = len(value)
        summary.append(item)
    result: dict[str, Any] = {"byte_length": len(data), "fields": summary}
    if errors:
        result["errors"] = errors
    return result


def digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def hex_value(value: bytes) -> tuple[str, bool]:
    if len(value) <= MAX_HEX_BYTES:
        return value.hex(), False
    return value[:MAX_HEX_BYTES].hex(), True


def known_hid_summary(method: str, direction: str, payload: bytes) -> dict[str, Any] | None:
    if method not in HID_METHODS:
        return None
    fields, errors = decode_wire_fields(payload)
    result: dict[str, Any] = {
        "message_type": "SendMsg" if method in SEND_METHODS and direction == "request" else (
            "ReadMsg" if method in READ_METHODS and direction == "request" else (
                "ResSend" if method in SEND_METHODS else "ResRead"
            )
        ),
        "protobuf_bytes": len(payload),
    }
    for number, wire_type, value in fields:
        if number == 1 and wire_type == 2 and method in HID_METHODS:
            if direction == "request" and method in SEND_METHODS:
                result["devicepath_length"] = len(value)
                result["devicepath_sha256"] = digest_bytes(value)
            elif direction == "request" and method in READ_METHODS:
                result["devicepath_length"] = len(value)
                result["devicepath_sha256"] = digest_bytes(value)
            elif direction == "response":
                # ResSend.err is intentionally metadata-only.
                result["error_present"] = bool(value)
                result["error_length"] = len(value)
        elif number == 2 and wire_type == 2 and direction == "request" and method in SEND_METHODS:
            encoded, truncated = hex_value(value)
            result["msg_length"] = len(value)
            result["msg_hex"] = encoded
            result["msg_hex_truncated"] = truncated
        elif number == 2 and wire_type == 2 and direction == "response" and method in READ_METHODS:
            encoded, truncated = hex_value(value)
            result["msg_length"] = len(value)
            result["msg_hex"] = encoded
            result["msg_hex_truncated"] = truncated
        elif number == 3 and wire_type == 0 and direction == "request" and method in SEND_METHODS:
            result["checksumtype"] = value
        elif number == 4 and wire_type == 0 and direction == "request" and method in SEND_METHODS:
            result["dangledevtype"] = value
    if errors:
        result["protobuf_errors"] = errors
    return result


def payload_summary(method: str, direction: str, payload: bytes) -> dict[str, Any]:
    known = known_hid_summary(method, direction, payload)
    if known is not None:
        return known
    return generic_wire_summary(payload)


def summarize_http_message(message: HttpMessage, direction: str, method: str) -> dict[str, Any]:
    content_type = message.headers.get("content-type", "")
    frames, errors = grpc_frames(message.body, content_type)
    frame_summaries: list[dict[str, Any]] = []
    for frame in frames:
        item = {key: value for key, value in frame.items() if key != "payload"}
        if frame["kind"] == "message":
            item["payload_summary"] = payload_summary(method, direction, frame["payload"])
        frame_summaries.append(item)
    result: dict[str, Any] = {
        "start_line": message.start_line,
        "framing": message.framing,
        "http_complete": message.complete,
        "content_type": content_type,
        "body_bytes": len(message.body),
        "grpc_frames": frame_summaries,
    }
    if errors:
        result["grpc_errors"] = errors
    return result


def method_from_request(message: HttpMessage) -> str:
    parts = message.start_line.split()
    if len(parts) < 2:
        return "<malformed>"
    path = parts[1].split("?", 1)[0]
    return path.rsplit("/", 1)[-1] or path


def capture_connection_id(path: Path) -> str:
    match = re.fullmatch(r"rpc-(.+)-request\.bin", path.name)
    return match.group(1) if match else path.stem


def parse_capture_pair(request_path: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    response_path = request_path.with_name(request_path.name.replace("-request.bin", "-response.bin"))
    request_data = request_path.read_bytes()
    response_data = response_path.read_bytes() if response_path.exists() else b""
    request_messages, request_trailing = parse_http_stream(request_data)
    response_messages, response_trailing = parse_http_stream(response_data)
    connection_id = capture_connection_id(request_path)
    exchanges: list[dict[str, Any]] = []
    count = max(len(request_messages), len(response_messages))
    for index in range(count):
        request = request_messages[index] if index < len(request_messages) else None
        response = response_messages[index] if index < len(response_messages) else None
        method = method_from_request(request) if request else "<unpaired>"
        exchange: dict[str, Any] = {
            "connection_id": connection_id,
            "exchange_index": index,
            "method": method,
        }
        if request:
            exchange["request"] = summarize_http_message(request, "request", method)
        if response:
            exchange["response"] = summarize_http_message(response, "response", method)
        exchanges.append(exchange)
    connection = {
        "connection_id": connection_id,
        "request_file": str(request_path),
        "response_file": str(response_path),
        "request_bytes": len(request_data),
        "response_bytes": len(response_data),
        "request_http_messages": len(request_messages),
        "response_http_messages": len(response_messages),
        "request_trailing_bytes": request_trailing,
        "response_trailing_bytes": response_trailing,
        "files_may_still_be_open": True,
    }
    return connection, exchanges


def offline_reference() -> dict[str, Any]:
    identity_path = DEFAULT_CAPTURE_DIR / "identity-bit7-read.json"
    keymap_path = DEFAULT_CAPTURE_DIR / "keymaps-initial.json"
    result: dict[str, Any] = {
        "identity_reference": str(identity_path),
        "keymap_reference": str(keymap_path),
    }
    if identity_path.exists():
        try:
            identity = json.loads(identity_path.read_text(encoding="utf-8"))
            replies = identity.get("replies", [])
            result["identity_read_reference"] = [
                {
                    "request_opcode": item.get("request", [None])[0],
                    "request_length": len(item.get("request", [])),
                    "reply_opcode": item.get("opcode"),
                    "reply_length": len(item.get("payload", [])),
                }
                for item in replies
            ]
        except (OSError, json.JSONDecodeError, TypeError, IndexError):
            result["identity_read_reference_error"] = True
    if keymap_path.exists():
        try:
            keymap = json.loads(keymap_path.read_text(encoding="utf-8"))
            result["keymap_reference_metadata"] = {
                "format_version": keymap.get("format_version"),
                "firmware": keymap.get("firmware"),
                "profile": keymap.get("profile"),
                "base_entry_count": len(keymap.get("base", [])),
                "function_entry_count": len(keymap.get("function", [])),
            }
        except (OSError, json.JSONDecodeError, TypeError):
            result["keymap_reference_error"] = True
    return result


def build_analysis(request_paths: Iterable[Path]) -> dict[str, Any]:
    connections: list[dict[str, Any]] = []
    exchanges: list[dict[str, Any]] = []
    for request_path in sorted(request_paths):
        connection, connection_exchanges = parse_capture_pair(request_path)
        connections.append(connection)
        exchanges.extend(connection_exchanges)

    method_counts: dict[str, int] = {}
    hid_exchanges: list[dict[str, Any]] = []
    for exchange in exchanges:
        method = exchange["method"]
        method_counts[method] = method_counts.get(method, 0) + 1
        if method in HID_METHODS:
            hid_exchanges.append(exchange)
    return {
        "format_version": 1,
        "tool": "Research/tools/rpc_analyze.py",
        "transport": {
            "listener": "127.0.0.1:3815",
            "target": "127.0.0.1:3814",
            "protocol": "HTTP/1.1 gRPC-Web",
        },
        "capture_files": connections,
        "rpc_method_counts": method_counts,
        "hid_rpc_methods": sorted(HID_METHODS),
        "hid_rpc_exchange_count": len(hid_exchanges),
        "hid_rpc_exchanges": hid_exchanges,
        "ordered_exchanges": exchanges,
        "offline_reference_comparison": {
            "captured_hid_rpc_present": bool(hid_exchanges),
            "note": (
                "No captured HID RPC exchange is available for direct comparison. "
                "The identity and keymap references below are offline protocol evidence only."
            )
            if not hid_exchanges
            else "Captured HID RPC payloads are summarized above; device paths are hashed.",
            **offline_reference(),
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "paths",
        nargs="*",
        type=Path,
        help="request capture files or directories; defaults to Research/captures/rpc-*-request.bin",
    )
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args()

    if args.paths:
        expanded: list[Path] = []
        for path in args.paths:
            if path.is_dir():
                expanded.extend(sorted(path.glob("rpc-*-request.bin")))
            elif path.name.endswith("-request.bin"):
                expanded.append(path)
        request_paths = expanded
    else:
        request_paths = sorted(DEFAULT_CAPTURE_DIR.glob("rpc-*-request.bin"))

    if not request_paths:
        parser.error("no rpc request captures found")

    analysis = build_analysis(request_paths)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        with args.output.open("x", encoding="utf-8") as output:
            json.dump(analysis, output, indent=2, sort_keys=False)
            output.write("\n")
            output.flush()
    except FileExistsError:
        parser.error(f"refusing to overwrite existing output: {args.output}")
    print(
        f"Analyzed {len(request_paths)} capture connection(s), "
        f"{len(analysis['ordered_exchanges'])} ordered exchange(s); wrote {args.output}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
