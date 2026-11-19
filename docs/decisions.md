# Design decisions

## 2026-11-19 — Register Sodium as a native extension

Register the PHP module and exception through the extension API. Use the existing
`base64` crate and the `hex` crate for the encoding functions. Match Sodium's
variant identifiers and ignore-string rules at the PHP boundary. The standard
extension's base64 decoder has different padding and invalid-input semantics.
The PHP oracle loads Sodium in both hosted test jobs so those semantics are
covered on Linux.
