# Check each mixed member and offset in a chain

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Keep the expression's byte range on checker findings during deduplication.
Two offsets on the same line can have identical messages and still be
separate findings. Baselines, text, and JSON continue to use source lines.

A property or method accessed on strict mixed produces implicit mixed.
Evaluate the arguments to an invalid call for their own findings. Check
each dimension of a mixed array write, including append dimensions. Infer
the numeric result of subtraction, multiplication, and division even when
an operand is mixed.

After a condition or a foreach refers to a mixed offset, track that offset
among the existing expression facts. A small arithmetic key uses its
expression structure as identity. A write to a variable in the key
invalidates the fact. Once the offset has a fact, later reads of it use
the refined value and the receiver and key are still checked.

The reference is PHPStan 2.3.7 at levels 9 and 10.

## Why

Each failed access is a distinct expression. A previously checked offset
can supply a more specific type while neighboring offsets still need checking.

## Turned down

Collapsing equal lines into one diagnostic or tracking arbitrary expressions
with side effects as offset keys.
