# Snapshot and fork is built in four steps: linked code shared per worker; a snapshot at `Phera\snapshot()` with a copy per request; adapters for Symfony and Laravel; copy on first write only if measurements ask for it (architecture.md 5.4)

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Owner asked for it; design

## Decision

Snapshot and fork is built in four steps: linked code shared per worker; a snapshot at `Phera\snapshot()` with a copy per request; adapters for Symfony and Laravel; copy on first write only if measurements ask for it (architecture.md 5.4).

## Why

One Laravel request costs 10 to 17 times PHP's time, and most of that is work repeated per request that opcache does once. Step 1 helps every application; the snapshot removes the framework's boot.

## Turned down

Offering only worker mode: state leaks between requests unless the application is written for it, so it cannot be the default for every application.
