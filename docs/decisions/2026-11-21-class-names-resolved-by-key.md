# Class names in the code are resolved by their process name key

- Date: 2026-11-21
- Area: Engine, classes
- Decided by: Claude (design), approved by Fable; the two instructions changed by Fable's decision

## Decision

- A static class reference (`new Post`, `Post::query()`, `instanceof Post`, `Post::LIMIT`) finds its class by the symbol's process name key: the worker keeps every shared entry of a name in two flat arrays (the first entry by key, the next by position), and the request answers whether it declared one. Built-in classes are a flat array by key. The per-symbol class slots each loaded file had (filled per request, copied per fork) are gone.
- A class name a script builds (`new $name`, `class_exists($name)`) is hashed once, in the worker, to its key, then takes the same path. Classes linked for one request alone and `class_alias` names are a short list of the request.
- `Post::LIMIT` compiles to `FetchNamedClassConstant`, which carries the class's symbol; `self::`, `parent::`, `static::` and run-time class names keep `FetchClassConstant`. A class not declared yet takes the string path out of line, which autoloads it.
- Files from the code cache get their metadata on the command line too, so command-line classes are found the same way.
- A file revision's class declarations are a flat table in the worker (by revision number, then class number).

## Why

The design rules ask for names resolved to indexes when code is linked, not hashed at run time. Whether a class exists stays the request's answer (its declared entries), so conditional declarations, aliases and two files declaring one name resolve per request.

## Turned down

- A string-keyed lookup cache in front of the class table.
- A flag inside `FetchClassConstant` instead of a second instruction: the existing handlers keep their code.
