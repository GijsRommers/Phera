# Classic requests do not share linked classes yet

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design

## Decision

Step 1 of architecture 5.4 is built as far as forks need it
(immutable classes, request state outside them). Sharing the linked classes
between classic requests of a worker, PHP's inheritance cache, is not built
in this change.

## Why

Function and class numbers are given per request in load order, so
a class linked in one request refers to code by numbers another request
does not share. Making them worker-wide changes `CodeTable::load`, which the
include-caching work is changing at the same time, and saves 45 to 60
million instructions per classic Laravel request, against 210 to 285 million
for a fork, which already starts with every class linked. An application
with a snapshot gets no benefit from it.

## Turned down

Doing it first, as 5.4 orders: the larger and more conflicting
change for the smaller gain.
