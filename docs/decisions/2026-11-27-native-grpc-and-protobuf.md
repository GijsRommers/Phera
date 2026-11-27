# Native gRPC server and client, and protobuf as PHP arrays

- Date: 2026-11-27
- Area: Server and requests
- Decided by: Design

## Decision

A new crate, `crates/extensions/grpc`, gives PHP `Phera\Grpc\Server`,
`Phera\Grpc\Client`, `Context`, `Status`, `StatusException`, and
`Phera\Protobuf\Schema` (see its README). It registers no PHP module name:
`extension_loaded('grpc')` stays false, because it is not PECL's API.

- **Messages are arrays, schemas are data.** A schema comes from a
  `protoc --descriptor_set_out` file, read with the wire reader itself, or
  from `Schema::define()`. There is no code generation.
- **Flat schema tables.** Messages and fields live in two vectors; messages
  refer to each other by index, fields are found by number through a
  64-entry direct table per message (binary search above 64). No hash map is
  consulted on the encode or decode path; PHP keys are made once per field.
- **Exact merge of repeated message fields.** A singular message field keeps
  its bytes until its message is read, and occurrences are concatenated
  before decoding. Merging decoded arrays would let the defaults of a later
  occurrence overwrite values of an earlier one.
- **`h2` directly, not hyper's server.** gRPC needs trailers, per-stream
  flow control and a trailers-only response; `h2` (hyper's own HTTP/2
  layer, already in the tree) exposes them directly, without hyper's
  executor and body types.
- **The server runs in the script's thread, without a `LocalSet`.** The
  driver polls connection and stream futures in a `FuturesUnordered` of its
  own; a worker future runs the PHP handlers. Both are joined inside
  `listen()`, so the handlers get the request's `Context` and nothing has to
  be `Send` or spawned.
- **Requests are read in full before PHP runs**, for unary, server-streaming
  and client-streaming calls alike; responses are written as PHP produces
  them, with back-pressure from HTTP/2 flow control.
- **Graceful stop.** `stop()`, SIGINT or SIGTERM stop accepting, send GOAWAY,
  and let running calls finish within `shutdownTimeoutMs`. Signal handling
  can be turned off (`handleSignals => false`), since the handlers stay
  installed after `listen()` returns.
- **Client channels are pooled per worker thread**, one HTTP/2 connection per
  endpoint, its driver spawned on the thread's runtime; a `Client` from
  before `Phera\snapshot()` keeps its configuration in each fork.
- **gzip with `flate2`** on whole messages, not `async-compression`: a gRPC
  message is one buffer, there is no stream to compress.
- **One addition to the extension API:** `Context::thrown_object()`, so
  native code can read the exception a PHP callback threw and answer with
  its status instead of unwinding. The VM implements it by looking into its
  `Thrown` payload; it is not on any hot path.

## Why

gRPC services and clients are common in PHP back ends, and PHP's own route
there (PECL `grpc` and `protobuf`, both C++ with generated classes) does not
fit Phera. Calling the handler directly in the thread that runs the script
removes the process hop that RoadRunner-style gRPC servers have.

## Turned down

- **`tonic` and `prost`.** Large dependency trees, code generation at build
  time, and `Send` bounds that PHP values (`Rc`) cannot meet.
- **Generated PHP message classes.** Arrays need no build step and decode
  without allocating objects; a class layer can be added on top in PHP.
- **Bidirectional streaming and incremental client streams** in this first
  version: they need a PHP-visible stream object with suspension points.
- **Cancelling a handler at its deadline.** Dropping a running PHP call mid
  way is not safe; the handler can read `deadlineMs()`, and a late unary
  answer is replaced by `DEADLINE_EXCEEDED`.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Protobuf schema | none | `Vec<MessageDef>` + `Vec<FieldDef>`, `direct: [u16; 64]` per message, sorted `by_name: Vec<u32>` and `methods` |
| Decode state | none | `Vec<Slot>` per nesting depth, reused; PHP array built once with `PhpArray::build_distinct` |
| Encode buffer | none | one `Vec<u8>` per message or frame, one reserved length byte per nested message |
| gRPC frames in | none | `Deframer`: zero-copy `Bytes` slices of DATA chunks, a `BytesMut` only for split frames |
| Server routes | none | sorted `paths: Vec<Box<str>>` with parallel `kinds` (stream tasks) and `Vec<Route>` with handlers (worker) |
| Client channels | none | thread-local `Vec<(key, SendRequest)>`, scanned linearly |
| Extension API | no way to read a thrown PHP exception | `Context::thrown_object(&NativeError) -> Option<Rc<Object>>` |
