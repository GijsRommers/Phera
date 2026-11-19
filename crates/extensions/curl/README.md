# PHP curl on Phera

Phera's curl extension implements the PHP 8.5 HTTP client API on Hyper and Tokio.
HTTPS uses rustls with ring. The client loads native trust roots off the worker
and falls back to WebPKI roots when the native store is empty. `CURLOPT_CAINFO`
and `CURLOPT_CAINFO_BLOB` replace those roots with PEM certificates.
`CURLOPT_CAPATH` adds certificates from a directory indexed by `openssl rehash`.
Client authentication accepts PEM or DER certificates and keys, supplied
as files or bytes. `CURLOPT_KEYPASSWD` decrypts encrypted PKCS#8 keys.
`CURLOPT_PINNEDPUBLICKEY` checks SHA-256 pins for the leaf public key in
addition to TLS verification. PKCS#12, traditional encrypted PEM private
keys, and file-based public-key pins remain unavailable.

The worker keeps up to 32 compatible client pools, each holding up to eight idle
connections per origin with a 45-second idle timeout. The pool key includes
routing, TLS settings, HTTP version, and proxy credentials. HTTP/2 connections
can multiplex transfers. File and stream operations yield to Tokio or the
runtime's stream I/O pool. PHP callbacks execute on the calling request task.

```php
$handle = curl_init('https://service.example/status');
curl_setopt_array($handle, [
    CURLOPT_RETURNTRANSFER => true,
    CURLOPT_TIMEOUT_MS => 1000,
]);
$body = curl_exec($handle);
if ($body === false) {
    throw new RuntimeException(curl_error($handle), curl_errno($handle));
}
```

[Option support](OPTIONS.md) lists the disposition of every PHP option. Setting
an unavailable option emits a warning, returns `false`, and leaves the transfer
configuration unchanged. TLS verifies the certificate chain and hostname by
default. An explicit `CURLOPT_SSL_VERIFYPEER => false` or
`CURLOPT_SSL_VERIFYHOST => 0` disables that check on the handle. The verbose
trace identifies the disabled checks, and configured pins still apply.
Proxy support covers HTTP forwarding and CONNECT. Other proxy types and
explicit proxies over a Unix socket are refused.

The client permits only HTTP and HTTPS, including on redirects. Other schemes
fail with `CURLE_UNSUPPORTED_PROTOCOL` before opening a file or a connection.
This removes non-HTTP protocol handlers from the request-forgery surface.
Application-controlled destinations and HTTP redirects still use the network
routing rules of the host.

## Compatibility and measurements

- `curl_version()` identifies Phera in the version string and rustls in the SSL
  field. Its numeric part is a cURL API compatibility floor for SDK version
  checks. Its feature bits and protocol list describe the Phera client.
- `curl_getinfo()` publishes the PHP 8.5 array keys and numeric selectors.
  Request and response header sizes are computed from Hyper's header view,
  including synthesized status lines. Case, ordering, framing bytes, and
  HTTP/2 wire representation can differ from libcurl.
- Connection identifiers belong to the Phera worker pool. DNS, TCP, TLS,
  response start, redirects, and total elapsed time use Phera measurements.
  The queue time is elapsed time before handing the request to Hyper, and the
  post-transfer time approximates when a connection was ready to send.
  The OS and backend error messages and certificate detail can differ from
  libcurl. Verification failures report a nonzero result. Certificate lists,
  proxy error details, native TLS engine lists, and default CA path selectors
  have empty or default results.
- Responses and uploads use bounded chunks. Hyper pulls from PHP streams and
  read callbacks as the network allows, and each disk-backed `CURLFile` part
  is read as it is sent. Multipart headers, string fields, and `CURLStringFile`
  values remain in memory. As in PHP, a consumed PHP upload stream cannot be
  rewound when a redirect requires retransmission. Callback chunk sizes and
  progress notification cadence can differ from libcurl.
- `curl_multi_select()` waits for progress or its deadline. Host and total
  multi limits cap active transfers. They are conservative for multiplexed
  HTTP/2, where several transfers can use one socket. The multi idle limit is
  bounded by the worker's pool limit.
- Cookie files use Netscape format. A handle writes its cookie jar when a
  transfer finishes or when an application sends `COOKIELIST` `FLUSH`.
  PHP normally writes its jar when the handle is freed.
- Share handles can share cookies, DNS, and a connection-pool namespace.
  Persistent handles share the DNS cache and pool by normalized option set.
  Persistent cookie sharing is rejected. Dedicated TLS-session sharing
  remains unavailable.
- `curl_upkeep()` has the PHP signature and return value for an idle handle.
  Hyper manages pooled connection lifetimes. Explicit libcurl upkeep probes
  and complete send/receive pause semantics remain unavailable.
- Direct construction, integer casts, cloning, and serialization restrictions
  are implemented for the relevant classes. Strict native dynamic-property
  diagnostics and reflection of function return unions still differ.

## Verification

`cargo test -p phera-ext-curl --test php_oracle -- --nocapture` runs the same
PHP scripts in both runtimes. Its bundled Hyper server binds loopback TCP and
Unix sockets and uses an rcgen certificate for TLS. The test matrix includes
binary escapes, PHP diagnostics, redirects, streamed responses, compressed
bodies, truncated transfers, timeouts, credential boundaries, proxy tunnels,
and concurrent requests. It inspects the oracle's underlying libcurl
capabilities when the PHP array shape depends on the installed library.
