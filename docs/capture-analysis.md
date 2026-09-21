# Captured official RPC analysis

`Research/tools/rpc_analyze.py` parses the opaque proxy streams offline. It
handles persistent HTTP/1.1 requests, `Content-Length`, chunked responses,
gRPC-Web text base64 frames, and generic protobuf wire fields. The analyzer
uses stdlib-only Python and writes with exclusive creation. It redacts all
non-HID protobuf values, hashes HID device paths, and keeps HID payload bytes
because those are the protocol evidence needed for replay work.

The first complete output is
[`rpc-analysis-v4.json`](../Research/captures/rpc-analysis-v4.json). The
captures were already closed by the time this output was generated. The older
`rpc-analysis.json`, `rpc-analysis-v2.json`, and `rpc-analysis-v3.json` files
are earlier parser iterations; they are retained because existing capture
artifacts are never overwritten or deleted.

The parser found six proxy connections and 41 ordered HTTP exchanges. Method
counts were:

| RPC method | exchanges |
| --- | ---: |
| `getItemFromDb` | 6 |
| `getAllValuesFromDb` | 7 |
| `getAllKeysFromDb` | 4 |
| `insertDb` | 2 |
| `changeWirelessLoopStatus` | 3 |
| `setLightType` | 1 |
| `watchDevList` | 1 |
| `watchVender` | 1 |
| `sendMsg` | 8 |
| `readMsg` | 8 |

The HID RPC sequence is one `sendMsg` followed by a `readMsg` for each of
these opcodes. Every request and reply payload is 64 bytes. The table shows the
first eight bytes of each reply; the remaining bytes are zero in the captured
identity-style reads except where the visible fields occupy them.

| command opcode | reply first 8 bytes |
| ---: | --- |
| `0x80` | `80 00 01 00 00 00 00 7f` |
| `0x84` | `84 00 01 00 00 00 00 7b` |
| `0x85` | `85 00 00 00 00 00 00 7a` |
| `0x87` | `87 05 04 04 07 08 08 08` |
| `0x86` | `86 00 10 00 01 00 00 79` |
| `0x91` | `91 00 01 00 00 00 00 6e` |
| `0x92` | `92 78 00 78 00 58 02 58` |
| `0x97` | `97 00 00 00 00 00 00 68` |

The captured `sendMsg` requests contain only the opcode in the first byte and
zeros in the rest of `msg`; the protobuf `checksumtype` and `dangledevtype`
fields are omitted, so they have their default value zero. The associated
`readMsg` replies contain the populated 64-byte reports. This gives a useful
app/helper boundary observation: the request seen by the RPC proxy is not
necessarily the final report bytes observed by a direct HID capture.

The `0x80` and `0x85` replies match the two request/reply opcode pairs in
[`identity-bit7-read.json`](../Research/captures/identity-bit7-read.json):
both have 64-byte request and reply reports, and the returned checksum bytes
are `0x7f` and `0x7a`, respectively. The RPC capture has no raw USB transfer
layer, so this comparison establishes matching report content at the helper
reply boundary without proving which side calculated each checksum.

The analyzer reports the `SendMsg` fields as `devicepath_length`,
`devicepath_sha256`, `msg_length`, `msg_hex`, `checksumtype` when present, and
`dangledevtype` when present. `ResRead.msg` is similarly reported as
`msg_hex`; `ResSend.err` is reduced to presence and length. Database and other
non-HID messages retain only protobuf field numbers, wire types, and byte
lengths, so the analysis output does not reproduce the captured database
values.

The two watch RPC captures ended while their server streams were still open.
The parser preserves complete gRPC-Web frames already available and marks an
HTTP message with `http_complete: false` when no terminating zero chunk was
captured. This is expected for a long-lived watch call and does not affect the
16 HID exchanges above.

