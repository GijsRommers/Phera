# Infer Doctrine manager results from entity class strings

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use the entity class string passed to an ORM entity manager to infer
the entity or repository type. Take the custom repository class from
the entity attribute when it declares one.

## Why

The class string and the mapped repository are available in source.
The checker can use them without booting Doctrine or querying a
database.

## Turned down

Executing an object manager factory during analysis or assuming
arbitrary strings identify a mapped entity.
