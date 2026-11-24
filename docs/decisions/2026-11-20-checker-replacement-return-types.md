# Infer replacement results from the subject type

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

For string and regex replacements, use the argument type to choose a
string or array result. Retain regex null results for string subjects and
possible encoding detection failure for `mb_convert_encoding`.

## Why

A plain native stub describes every overload in a single return union.
Strict union checks need the result of the overload a call actually uses.

## Turned down

Checking the entire return union for calls whose subject type identifies
one alternative.
