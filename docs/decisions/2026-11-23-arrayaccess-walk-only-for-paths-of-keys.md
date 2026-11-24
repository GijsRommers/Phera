# The ArrayAccess walk runs only for paths of keys

- Date: 2026-11-23
- Area: General tier, paths
- Decided by: Design

## Decision

Before the general tier runs a path instruction, `Vm::prepare_dimension` walked the path looking
for an object with `ArrayAccess` at one of its keys, so that the instruction would run as a
dimension of that object. It ran as soon as any loaded class implemented `ArrayAccess`, which
on a Symfony request is always.

Now it walks only a path of arrays and keys (`$rows[$i]['id']`, `$config['db'][] = ...`). A path
with a property step (`$this->items[$key]`, `$this->a->b[$k] = $v`) is not walked:

- every handler of the general tier takes only paths with one property step and no key, and
  sends any other property path to the asynchronous tier (`dispatch`);
- that tier's walk checks each object it meets at a key itself and runs the dimension there
  (`magic_places`, `magic_mutations`, `magic_references`), as the walk would have.

**One exception: `++` and `--`.** On `$this->store['n']++` with an `ArrayAccess` store, PHP gives
"Indirect modification of overloaded element" and calls only `offsetGet`; the asynchronous tier
would write the element back with `offsetSet`. The increment keeps the walk
(`Vm::prepare_any_dimension`), and with it the dimension path, which gives PHP's notice.

## Why

Counted on the Symfony route (counting build of main 1fcc6383): the walk ran **10,836 times per
request and found an `ArrayAccess` object twice**. Each walk read the path's properties by name,
about 2,000 instructions.

Requests, through `quiet.sh`, against main 0c63b85f (record) and against main f9cf1e1f:

| route | 0c63b85f before | after | f9cf1e1f before | after |
| --- | ---: | ---: | ---: | ---: |
| Symfony `/api/db-yardstick` (3 pairs, `YARDSTICK_WARM=300`) | 433.28M | 399.31M (-7.84%) | 265.34M | 263.14M (-0.83%) |
| `/api/hello` (12 pairs) | 23.23M | 22.96M (-1.18%) | 20.80M | 20.70M (-0.48%) |
| `/api/posts?page=2` (12 pairs) | 104.73M | 103.34M (-1.32%) | 50.69M | 50.57M (-0.25%) |

On f9cf1e1f the per-class property table (#217) keeps most property paths in the loop, so far
fewer reach the general tier and its walk; the gain is what is left of it. The 20 benchmarks
are within +0.02% (lowest of five).

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| the test for an `ArrayAccess` object on a path | a process flag ("some class has it"), then a walk of the whole path with each property read by name | a path of keys: the walk, under the flag, as before; a path with properties: the object at hand, where the asynchronous walk meets it |

## Turned down

- **Dropping the flag for paths of keys as well.** The synchronous read would have to notice the
  object itself and hand over the rest of the path; the walk over arrays is cheap and the flag
  keeps it off requests that load no such class. Left for when the counts show those walks.
