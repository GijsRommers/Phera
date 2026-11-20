# Tokenize PHP when generating package class maps

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use Phera's tokenizer to read namespace and class, interface, trait and enum
declarations. Read source as bytes, as PHP does.
Keep declaration names as bytes until writing the class map. Encode names
outside UTF-8 as PHP hexadecimal string escapes.

## Why

Composer builds a class map before it runs the PHP files. Packages can contain
non-UTF-8 byte strings, source excerpts and syntax Phera cannot execute.
Finding named declarations only needs the tokens and works in those files.
Symfony Cache also uses a non-UTF-8 byte in a declared class name.

## Turned down

Parsing every PHP file and patching unsupported syntax before parsing it
again. An unrelated parse error could prevent an otherwise valid installation.
