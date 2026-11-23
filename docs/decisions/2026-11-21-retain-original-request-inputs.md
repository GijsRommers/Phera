# Retain the original request inputs

- Date: 2026-11-21
- Area: Input filters
- Decided by: Design

The filter extension retains shared references to the initial PHP arrays
for a request. Writing a superglobal separates its data, so input filtering
keeps seeing the original values. The HTTP server input exposes the
query string and incoming header values that PHP's built-in server gives
to input filters. It builds a filtered array only when PHP asks for the
whole input source.

When a request has no POST values, `$_REQUEST` and `$_GET` can share
their initial storage. PHP writes keep their ordinary copy-on-write
behavior.
