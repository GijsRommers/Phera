# The VM records a property site for reads of an element held in a property

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

When a read of `$this->items[$key]` (or of `$object->map['a']['b']`, quiet or
not) reaches the VM, it records the property's site as it does for a read
of the property alone; finding the entry raises nothing, and a property that
cannot be read from there gets no entry. The interpreter loop then reads the
property from the site and walks up to three integer or string keys itself.
A missing key in a plain read, hooks, `ArrayAccess` objects, string offsets
and PHP references stay with the VM. (`crates/vm/src/property_sites.rs`,
`crates/vm/src/calls/fast_property_elements.rs`)

## Why

This is the most frequent property shape in framework code (container
lookups, configuration, request bags), and it found the property by name on
every read. Recording the site is a pure lookup, so it changes no behaviour.

## Turned down

A separate cache for element reads (a second record of the same visibility
decision); walking keys of any type (key conversion warns for floats, bools
and null, which belongs to the VM).
