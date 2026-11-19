# Worker mode is supported too, through FrankenPHP's interface (`frankenphp_handle_request()`), so that Laravel Octane, Symfony's runtime packages and FrankenPHP applications run unchanged

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Owner

## Decision

Worker mode is supported too, through FrankenPHP's interface (`frankenphp_handle_request()`), so that Laravel Octane, Symfony's runtime packages and FrankenPHP applications run unchanged. Phera keeps many running copies of the worker script per thread, each answering one request at a time, so that waiting stays cheap.

## Why

The owner wants applications written for long-running use to run as they are. An existing interface means no new package for anyone to write.

## Turned down

A Phera-only worker interface; one copy of the worker script per thread, which would give up many requests per thread. This reverses the first draft of the design, which left worker mode out.
