# Keep dalek’s Ed25519 public-key validation

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Sign with dalek’s expanded secret key API so the stored public bytes contribute to the
signature. Reject keys whose stored public half cannot be decoded. Eight of 16 malformed
detached-signing probes differ from PHP.

## Why

Dalek’s signing API requires a decodable public point. Deriving a replacement from the seed
changes the transcript.

## Turned down

A custom Ed25519 signature implementation for malformed keys.
