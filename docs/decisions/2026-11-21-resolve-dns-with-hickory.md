# Resolve DNS with Hickory

- Date: 2026-11-21
- Area: Standard library
- Decided by: Design

## Decision

Forward host lookups use Tokio's system resolver. Reverse lookup first
checks the hosts file, then asks the configured DNS server for a PTR.
PHP's DNS record and MX functions use Hickory's asynchronous resolver
and the system DNS configuration. Raw DNS records preserve the RDATA
bytes from the response. Phera leaves Hickory's answer cache disabled.

Service and protocol lookups read the system files on demand. Network
interface enumeration uses nix's safe getifaddrs wrapper on a blocking
thread. The oracle verifies the parts of the result a program can rely
on across hosts.

## Why

These operations can wait on the operating system or on a name server.
They must leave the PHP worker available for other requests. The system
tables and DNS configuration can change over the lifetime of a worker.

## Turned down

Hard-coded tables of service names and protocol numbers, and a
Phera-specific cache of resolver answers.
