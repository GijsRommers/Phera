# Reuse Argon2 and implement Sodium’s scrypt password format

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Reuse the workspace Argon2 implementation. Translate the Sodium scrypt work factors into N, r
and p and use Sodium’s `$7$` encoding. Run password operations on the blocking pool.

## Why

PHP uses Sodium-specific defaults and memory units. Password hashes must verify in PHP and
Phera without blocking the VM worker.

## Turned down

Using scrypt’s PHC format for Sodium passwords.
