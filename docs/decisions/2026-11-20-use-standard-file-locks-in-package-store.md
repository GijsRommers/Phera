# Use Rust file locks for shared package storage

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use `std::fs::File::lock` to serialize changes to a package index or its
content-addressed directory.

## Why

Rust exposes the exclusive lock directly and releases it when the file closes.
The package store holds the locked file for the same scope as before.

## Turned down

Using a dependency for the two calls that lock package storage.
