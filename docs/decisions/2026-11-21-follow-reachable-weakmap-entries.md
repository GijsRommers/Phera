# Follow WeakMap entries when the map and key are reachable

- Date: 2026-11-21
- Area: Value lifetime
- Decided by: Design

## Decision

Keep WeakMap values attached to their keys so dropping an ordinary key
releases its value immediately. On a collection, count each attachment as
an internal owner of its value. Follow that edge only when both the map
and the key are reachable.

Register the edge at each endpoint in flat lists built for the current
collection. Marking either endpoint can activate the edge, so the work
list reaches a fixed point even when several maps or keys depend on one
another. Break attachments on surviving keys when their map is garbage.

WeakMap and WeakReference native state can be skipped during traversal
because it holds weak PHP handles. Other native state remains opaque.

## Why

A key used as its own entry value must be collected. A key reached through
an entry of a live map must continue to activate its own entries. These
conditions can be evaluated using the collector's existing node indexes
and reference counts. Programs with no WeakMap values need no dependency
storage.

## Turned down

Treating all attachments as ordinary strong edges, or rescanning all maps
after each newly reached value.
