# A property's class types are keyed by lowercased name

- Date: 2026-11-23
- Area: Properties, interpreter loop
- Decided by: Design

## Decision

The per-class property table (`classes::property_table`) keeps, for each class-typed property, the keys of the classes its declared type names, so that the loop can check an object written into it (`PropertyTable::allows_object_class`). Those keys are now made from the lowercased names, as every class key is (`phera_bytecode::name_keys`, `ClassTable::by_key`).

## Why

The names were keyed as written (`Connection`). Class keys number lowercased names, so the rule's key never matched a class and `allows_object_class` said no to every object. Every object written into a class-typed property (`private Connection $c`, `?Named $x`, `A|B $y`) went to the general tier, since the table replaced the site check (#222). A counting build showed it: each such write was refused by the class check.

Instructions per new object, a constructor with one promoted `Connection` parameter (`~/.phera-review/plans/readonly.php`):

| main | instructions |
| --- | ---: |
| f9cf1e1f (site check) | 2,470 |
| e6f235b80 (table, before) | 3,649 |
| this | 2,515 |

Requests against main e6f235b80, through `quiet.sh`:

| route | before | after |
| --- | ---: | ---: |
| Symfony `/api/db-yardstick` (3 pairs, `YARDSTICK_WARM=300`) | 264.98M | 262.99M (-0.75%) |
| `/api/hello` (12 pairs) | 19.30M | 19.30M |
| `/api/posts?page=2` (12 pairs) | 48.90M | 48.91M |

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| a class type rule's keys | keys of the names as written, matching no class | keys of the lowercased names, the classes' own keys |
