# Run loaded Reflection constructors synchronously

Date: 2026-11-21

## Decision

A native method can offer a synchronous handler for arguments that already match its signature. The Reflection constructors use it for plain positional calls. For example, `new ReflectionClass($object)` reads the object's linked class and `new ReflectionClass(Service::class)` looks up the supplied name. Both bind the reflector without allocating a future.

A class miss restores the arguments to the prepared call. The ordinary handler then runs the existing autoload sequence and reports the same Reflection exception if the class stays missing. Named arguments, coercions and the deprecated one-argument form of `ReflectionMethod` also use the ordinary binder.

## Why

The metadata of a linked class is already available when the constructor runs. The constructor needs to wait only when loading may call PHP. A linked method can use the resolved class number when it selects its signature and when it later invokes the method.

| Structure | Was | Is |
| --- | --- | --- |
| Native method entry | Sync or async handler and optional argument preparation | An optional ready handler alongside the existing handler and preparation |
| Prepared positional constructor arguments | A vector passed to the async method | Two stack slots on the ready path, restored to the call on a miss |
| Reflection class input | Owned class name, including for objects | Linked class for objects and a borrowed name for valid UTF-8 strings |
| Class name during autoload and enum case lookup | Owned string for each name lookup | Borrowed object name or valid UTF-8 string, with owned bytes for lossy conversion |
| Method constructor target | Class and method names | Linked class number and method name, with the existing function metadata map keyed by class number and folded method name |
| Class reflector name | A new PHP string written during binding | The linked class's PHP string shared with the reflector |

Each reflector carries boxed state. When a worker forks from a snapshot, the class number identifies the same class table position used by class and member reflectors.

The borrowed name on the general lookup path incorporates the approach proposed in PR #163.

## Turned down

- Memoizing constructor calls. Each call still creates a separate PHP object.
- Retrying the constructor as an ordinary failed class-fetch instruction. The ordinary Reflection handler owns the autoload order and the exception for a missing class.
