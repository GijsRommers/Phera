# Every extension says how a fork gets its request state

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

`RequestState` remembers each slot's type name, and a fork
copies each slot with the policy its extension registered
(`Registry::fork_state`): `fresh_state` (the fork starts without it, as a
new request does: response headers, uploads, json, pcre, libxml and openssl
errors, random number seeds, serializer and reflection caches),
`copy_state` (a setting the boot made: timezone, locale, working directory,
mbstring and iconv settings, `openlog()`), or a function that copies PHP
values inside (the `WeakReference` registry, libxml's entity loader, a
session save handler, the stream table). A type without a policy refuses
the snapshot by name. A session started before the call is refused.

## Why

The VM cannot know what extensions keep; a default of "start
fresh" or "copy" would be silently wrong for some type. Refusing by name
makes a forgotten type visible the first time a snapshot holds it.

## Turned down

A trait bound on every state type (changes every call site of
`get_or_default`).

## Batches 12 to 14

Objects whose state is plain data (GMP, `BcMath\Number`, intl formatters,
collators, calendars, time zones, iterators, transliterators, resource
bundles) fork as clones (`fork_clone`). Objects that hold a connection, a
transfer or a compression in progress (curl handles, multi and share
handles, mysqli and its results and statements, pgsql, zlib contexts, gzip
streams) refuse, as PDO does (`fork_refused_handle`). libxml, intl and zlib
output state start fresh; gmp's random state starts fresh, so a seed set
before the snapshot is not kept (as for `mt_rand`); intl's default locale is
an INI value and is copied with the INI.
