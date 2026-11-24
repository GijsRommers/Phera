# Report absent offsets in null checks of closed array shapes

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

A literal offset outside a closed array shape cannot affect the result of
`isset` or `??`. Report the missing offset with that operator's
identifier. An open shape can contain extra keys.
