# gRPC and Protocol Buffers

`Phera\Grpc\Server`, `Phera\Grpc\Client` and `Phera\Protobuf\Schema`: gRPC
over HTTP/2 and the protobuf wire format, written in safe Rust on `h2`,
`tokio`, `rustls` and `flate2`. No `protoc` plugin, no generated PHP classes,
no PECL `grpc` or `protobuf` extension: messages are PHP arrays.

```php
use Phera\Grpc\{Client, Context, Server, Status, StatusException};
use Phera\Protobuf\Schema;

$schema = Schema::fromDescriptor(file_get_contents('echo.pb'));
// protoc --include_imports --descriptor_set_out=echo.pb echo.proto
// or, without protoc:
$schema = Schema::define([
    'echo.Req'  => [1 => ['name' => 'message', 'type' => 'string'], 2 => ['name' => 'count', 'type' => 'int32']],
    'echo.Resp' => [1 => ['name' => 'message', 'type' => 'string'], 2 => ['name' => 'index', 'type' => 'int32']],
], [
    'echo.Echo' => ['Say' => ['echo.Req', 'echo.Resp']],
]);

// server.php: phera server.php
(new Server('0.0.0.0:50051', $schema))
    ->unary('/echo.Echo/Say', fn (array $req, Context $ctx) => ['message' => $req['message']])
    ->serverStream('/echo.Echo/Count', function (array $req, Context $ctx) {
        for ($i = 1; $i <= $req['count']; $i++) {
            yield ['message' => $req['message'], 'index' => $i];
        }
    }, 'echo.Req', 'echo.Resp')
    ->enableHealthService()
    ->listen();

// client.php
$client = new Client('127.0.0.1:50051', $schema);
$client->unary('/echo.Echo/Say', ['message' => 'hi']);   // ['message' => 'hi', 'index' => 0]
try {
    $client->unary('/echo.Echo/Say', ['message' => 'hi'], timeoutMs: 50);
} catch (StatusException $e) {
    echo Status::name($e->getCode()), ': ', $e->grpcMessage;   // DEADLINE_EXCEEDED: Deadline Exceeded
}
```

## Messages

| PHP | protobuf |
| --- | --- |
| `int` | `int32`, `int64`, `uint32`, `sint32`, `sint64`, `fixed32`, `sfixed32`, `sfixed64`, `enum` |
| `int` (two's complement above `PHP_INT_MAX`; a decimal string is accepted when encoding) | `uint64`, `fixed64` |
| `float` | `double`, `float` |
| `bool` | `bool` |
| `string` (UTF-8 checked for `string`) | `string`, `bytes` |
| `array` (a `stdClass` is accepted when encoding) | message |
| list | `repeated` (numbers packed, or not, as declared; both are read) |
| `array` keyed by the map key | `map<K, V>` |

Decoding gives every field without presence, at its default when absent, and
every repeated or map field (`[]` when absent); a field with presence (a
message, a oneof member, `optional`, proto2) only when the message carried
it. Encoding writes fields in number order, leaves proto3 defaults out,
treats `null` as unset, and rejects a key that is not a field. Unknown fields
on the wire are skipped. A message field that occurs twice is merged, as
protobuf requires.

`Schema::define()` takes the same information as a `.proto` file: per field
`name` and `type` (a scalar name, `map` with `key` and `value`, or a message
name resolved as in a `.proto` file), and optionally `repeated`, `packed`
(default true) and `optional`. Its second argument declares services, which
lets routes and calls leave out their message types.

## Server

`new Server(string $address = '127.0.0.1:50051', ?Schema $schema = null, array $options = [])`

- `unary`, `serverStream`, `clientStream(string $method, callable $handler, ?string $requestType = null, ?string $responseType = null)`.
  Without message types (given, or declared by the schema's service) the
  handler receives and returns the message bytes.
  - unary: `fn (array|string $request, Context $ctx): array|string|null`
  - server streaming: returns an array of messages or any iterable; a
    `Generator` is consumed one message at a time, each on the wire before
    the next is produced.
  - client streaming: `fn (array $requests, Context $ctx)`, with every
    request message once the client has finished sending.
- `enableHealthService()`: `grpc.health.v1.Health/Check` answers `SERVING`
  for the server and each service with a route, `NOT_FOUND` otherwise,
  without running PHP.
- `listen()` serves until `stop()` is called (from a handler), or SIGINT or
  SIGTERM arrives; it then stops accepting, sends GOAWAY, lets running calls
  finish (at most `shutdownTimeoutMs`) and returns.
- Options: `tls` (`['cert' => 'cert.pem', 'key' => 'key.pem']`, ALPN `h2`),
  `maxMessageBytes` (default 4 MiB, larger is `RESOURCE_EXHAUSTED`),
  `compression` (`'gzip'`: compress responses for clients that accept it; a
  gzip request is always answered with gzip), `handleSignals` (default
  true), `shutdownTimeoutMs` (default 10000).

A `StatusException` thrown by a handler becomes its status, message and
metadata (as trailers). Any other exception becomes `UNKNOWN` with the
exception's message, and is reported on stderr. An unknown method is
`UNIMPLEMENTED`; a call whose `grpc-timeout` has passed before its handler
runs, or before a unary response is written, is `DEADLINE_EXCEEDED`.

`Context`: `method()`, `metadata()` (request metadata without the protocol's
own headers; repeated names joined by `,`), `header(string $name)` (any
request header), `setHeader()` (before the first message),
`setTrailer()`, `deadlineMs()` (milliseconds left, or null), `peer()`.
Metadata names ending in `-bin` carry bytes; they are base64 on the wire only.

## Client

`new Client(string $endpoint, ?Schema $schema = null, array $options = [])`,
with `host:port`, `http://host:port` or `https://host[:port]`.

- `unary(string $method, string|array $request, ?string $requestType = null, ?string $responseType = null, array $metadata = [], ?int $timeoutMs = null): string|array`
- `serverStream(...)`: the list of response messages.
- `clientStream(string $method, array $requests, ...)`: one response.
- `lastHeaders()`, `lastTrailers()`: the metadata of the last call.
- `close()`: drops the pooled connection.
- Options: `tls` (TLS without `https://`), `caFile` (extra trusted
  certificates, PEM), `authority`, `compression` (`'gzip'`), `metadata`
  (sent with every call), `timeoutMs`, `connectTimeoutMs` (default 10000),
  `maxMessageBytes`.

A status other than `OK` throws `StatusException` with the server's message
and trailers. An expired timeout is `DEADLINE_EXCEEDED` (and resets the
stream), a refused connection `UNAVAILABLE`, an HTTP error status the code
gRPC maps it to.

Connections are pooled per endpoint and worker thread: every `Client` and
every request on the thread multiplexes its calls over one HTTP/2
connection. A call waits without blocking: in `phera serve`, other requests
on the worker run meanwhile. A `Client` made before `Phera\snapshot()` works
in every forked request.

## Inside

```text
messages  [MessageDef]  name · fields first..first+count · direct[64]
fields    [FieldDef]    number · kind · flags · message index · PHP key
by_name   [u32]         message ids sorted by name
methods   [MethodDef]   sorted by "/pkg.Service/Method"
```

- A schema is flat tables; messages refer to each other by index. A field
  is found by number through its message's 64-entry direct table, or by
  binary search above 64. Its PHP array key is made once. No hash map is
  used while encoding or decoding.
- Encoding writes into one buffer, reserving one length byte per nested
  message and widening it (one move) only for messages of 128 bytes or more.
  The server and client encode straight after the five gRPC header bytes, so
  a frame is a single buffer handed to HTTP/2.
- Decoding collects each field in a slot of a table indexed by the field's
  position (tables reused per nesting depth), and builds the PHP array once,
  with keys known to be distinct. Frames are cut out of HTTP/2 DATA chunks
  without copying when a chunk holds whole frames.
- The server's `listen()` is one future on the script's thread: a driver
  polls the sockets, connections and streams as a set of its own, and a
  worker runs PHP handlers for calls the streams have fully read, one at a
  time, writing each response onto its stream (waiting on HTTP/2 flow
  control). Both run together, so streams move while PHP runs.
- Not supported: groups (proto2), extensions, `Any` and well-known-type
  JSON mapping, bidirectional streaming, streaming client responses as they
  arrive, and compressions other than gzip.
