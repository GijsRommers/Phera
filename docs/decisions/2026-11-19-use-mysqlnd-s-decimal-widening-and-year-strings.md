# Use mysqlnd's decimal widening and year strings for mysqli and capture PostgreSQL prepared-result field sizes on the existing socket

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use mysqlnd's decimal widening and year strings for mysqli and capture PostgreSQL prepared-result field sizes on the existing socket.

## Why

Wire types and PHP-visible types differ at these boundaries. PDO's conversion policy remains independent.

## Turned down

Changing the PDO defaults or inferring PostgreSQL field sizes from column names.
