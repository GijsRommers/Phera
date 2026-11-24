# DBGp oracle

These sessions were recorded with PHP 8.5.12 and Xdebug 3.5.0. The Rust peer
uses the command files for both engines. Normalization preserves the XML
elements and decoded attributes, including each transaction ID. It
canonicalizes XML spelling, local paths, process IDs, and breakpoint IDs
allocated by the engine. The test checks Phera's init identity separately.

The sessions cover a line breakpoint, stack and variable inspection,
assignment, evaluation, and stepping. They also cover a generator from an
include, a conditional breakpoint, caught and uncaught exceptions, enum
cases, a native date object, a lazy ghost, and a recursive array. Stream
controls and boot-include breakpoints have separate Phera tests.

To refresh the recorded packets with a local Xdebug installation, set
`PHERA_RECORD_DEBUG_ORACLE` to the PHP ini file that loads Xdebug and run
`cargo test -p phera-cli --test debugger`. The peer starts PHP with that
file and a clean environment. Ordinary test runs use the recorded packets.

The following results differ in additional debugger probes:

- Phera reports `supports_async=1` because DBGp `break` works while PHP
  runs. Xdebug 3.5.0 on this Mac reports `0` on the DBGp connection.
- Phera reports DBGp error 206 for a failed eval. Xdebug 3.5.0 also includes
  the underlying PHP diagnostic in the protocol message.
- The exception wildcard stops when the Throwable is thrown in both
  engines. Xdebug can stop a second time for the fatal diagnostic emitted
  after an uncaught exception. Phera has one exception event for the throw.
