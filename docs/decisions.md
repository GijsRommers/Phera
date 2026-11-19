# Implementation decisions

## Native parameters and exception frames

PHP's pinned stubs supply default expressions for skipped native argument slots and reflection. Native handlers keep their positional arity and coercion behavior. A skipped UNKNOWN slot raises an argument count error.

A native call with a PHP-visible frame supplies that frame when it returns a request for a fresh Throwable. This covers both ordinary errors and errors with extra properties or a custom code. A Throwable created by a nested PHP callback keeps the trace captured at creation. Frameless compiler instructions and VM continuations follow their own call semantics.

Synchronous handlers keep their original arguments until they return. Async handlers take ownership of the arguments, so the VM retains their values in a reusable buffer. The exception frame and its argument values are materialized on failure. Native operations that publish their own callback frame keep that frame while the callback runs.
