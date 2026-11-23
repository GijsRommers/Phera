# A declaration whose worker entry waits for its parent raises the miss itself, and declaring is keyed

- Date: 2026-11-22
- Area: Classes
- Decided by: Design

## Decision

**1. The open finding, and its fix.** A request often declares a class before what it extends: the autoloader loads `Manager` before its trait, `Child` before its parent or interface. The worker's entry for the class then lists a dependency that was a class when the entry was made and is no class yet in this request. Before, the reuse check found the entry invalid, and the class was linked until the link missed that dependency. The dependency was autoloaded, the declaration retried and reused the entry. That aborted link ran in every request.

Now the check raises the miss itself, when two things hold:
- an entry's recorded lookups match up to the first dependency that is now missing;
- that dependency is the declaration's own parent, interface or trait.

The entry was linked through that same prefix of lookups, so a link now would make the same lookups and miss at that name. The miss has the same name and kind (`Class`, `Interface`, `Trait`) as the link's. So the autoloader is called for the same names, in the same order, and the errors are the same.

**2. Dependencies by key.** A worker entry's dependencies are `(NameKey, Option<u32>)`, and checking one is `ClassTable::by_key`: one keyed lookup, exact as before. The key is made while linking records the lookup.

**3. Declaring by key, and order without names.**
- A compiled file carries the key of each class it declares (`UnitMetadata::class_keys`), so `declare_class` looks the name up by key: no lowercasing, no string hash.
- `get_declared_classes`'s order holds what was declared (a position, or an alias's index) instead of a copy of each name.

## Why

Measured on main b464e720 with class warm-up off (`phera serve --no-class-warmup`, one worker, after 150 requests). Events per request are counted by a scratch build that logs each declaration's path.

| route | reused | aborted link attempts | early misses | full links |
| --- | ---: | ---: | ---: | ---: |
| `/api/hello` before | 70 | 24 | — | 0 |
| `/api/hello` after | 70 | 0 | 24 | 0 |
| Symfony route before | 309 | 125 | — | 5 |
| Symfony route after | 309 | 13 | 112 | 5 |

What remains on the Symfony route:
- **The 5 full links are anonymous classes**, which are each request's own by design.
- **Of the 13 aborted attempts, 2 are anonymous classes.** The other 11 wait for a class that the link looks up while checking method signatures against the parent's (a parameter or return type). Linking misses those through the signature path, which keeps the linked shape and resumes it. Raising that miss early would need that shape, so they stay as they are.

Instructions per request, class warm-up off, 8 pairs:

| route | before | after |
| --- | ---: | ---: |
| `/api/hello` | 31.06M | 30.58M (-1.57%) |
| `/api/posts?page=2` | 121.00M | 118.66M (-1.93%) |

Command line, instructions per run (PHP 8.5 `-n` for comparison):

| command | PHP | before | after |
| --- | ---: | ---: | ---: |
| `artisan list` | 1.36G | 3.39G | 3.30G (-2.8%) |
| `artisan route:list` | 1.34G | 3.19G | 3.13G (-2.0%) |

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| a worker entry's dependency | `(String, Option<u32>)`, checked by lowercased name through the worker's string map | `(NameKey, Option<u32>)`, checked with `by_key` |
| an entry that waits for its parent, interface or trait | found invalid; the class linked until the miss, then retried | the miss raised by the check; the retry reuses the entry |
| a declaration's name | lowercased and hashed on every `DeclareClass` | its key from `UnitMetadata::class_keys` |
| declaration order (`get_declared_classes`) | an `Rc<str>` copy of each name | `Declared::Class(position)` or `Declared::Alias(index)` |

## Turned down

- **Comparing one number per dependency instead of a lookup.** A name can map to different entries, and a dependency may be unresolved: the lookup stays.
- **Raising the signature-path miss early.** It resumes a kept shape; see above.
- **Stopping `declare_linked`'s per-method writes.** That needs the fork-shared code table (`speed/fork-shares-code`); the interface is written in `~/.phera-review/hello-cost/INTERFACE-function-facts.md`.
