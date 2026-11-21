# A path reads a local key in place instead of a copy made just for the read

- Date: 2026-11-21
- Area: Compiler
- Decided by: Design

## Decision

Where the compiler emitted `Move temp, local` only so that a later path
instruction could read the key, the path step now names the local register
directly (`$row[$key]` is `FetchIndex { index: $key }`), and the `Move` and the
`Clear` of its temporary are gone. A copy is kept when any of these holds:

- the local may hold a PHP reference at run time: a by-reference parameter or
  capture, `global`, `static`, a `&` binding (`MakeReference`, `BindReference`,
  `foreach ... as &`), or a place the local roots that is sent by reference
  (`SendPath`), or passed to a call that may take it by reference (`SendLocal`,
  a direct call's argument area). The scalar loop falls back to the general
  path on a reference slot, so reading such a local in place would leave the
  loop on every execution.
- the function has a dynamic scope (`$$name`, `extract`, `compact`, `include`,
  `eval`, top-level code): any register can be bound to a reference through the
  name table, so every local keeps its copy.
- anything between the copy and the consumer can write the local: an
  instruction that names it as a destination, or (with dynamic scope) one that
  can run PHP code.
- the temporary has any reader besides the consuming path instruction, or more
  than one write, or a jump, branch target or exception-handler entry lies
  between the copy and the consumer, so that not every path reaches the
  consumer through the copy.

Reading an undefined local key still warns exactly once per evaluation: the
warning runs where the path resolution turns a step's register into a key
(`path_index_key`, `magic_path_key`, `keys`, `property_keys_for`,
`unset_index_key`, the overloaded-dimension divert arms), and on the skip
paths where the container cannot be indexed but PHP still evaluates the
offset. Only a `None` slot pays for the frame lookup, and only a named local
can warn, so a defined key pays one branch and temporaries never warn. On a
read with both base and key undefined the warning order is base before key,
which is PHP's order; `main` warned the key first because the copy did.

## Why

41% of all executed `Move` bytecodes on the Symfony yardstick were a local
copied into a path key register, and every such copy brought a `Clear` with it.
A local is a register already; the copy exists only so that a later operand of
the same expression (`$a[$i][$i++]`) cannot change an earlier one, which the
checks above preserve.

## Turned down

- Taking the value on the last read (removing the copy and the release together
  by changing what instructions do to their operands) belongs to the
  interpreter loop, not the compiler; it is scheduled with the loop owners.
- Rewriting a temporary with several writes (as `$a[$k] ??= 1` produces) needs
  a control-flow argument the single-write check avoids; those copies stay.
