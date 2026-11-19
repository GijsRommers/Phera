--TEST--
Stream chunk size and capabilities for supported wrappers
--FILE--
<?php
foreach ([__FILE__, 'php://memory', 'php://temp', 'data://text/plain,content'] as $name) {
    $stream = fopen($name, 'r');
    var_dump(stream_set_chunk_size($stream, 1024), stream_set_chunk_size($stream, 32));
    var_dump(stream_supports_lock($stream), stream_is_local($stream), stream_isatty($stream));
    var_dump(stream_set_read_buffer($stream, 0), stream_set_write_buffer($stream, 8));
    var_dump(stream_set_timeout($stream, 1));
    fclose($stream);
}
$stream = fopen('php://memory', 'r+');
foreach ([0, PHP_INT_MAX] as $size) {
    try { stream_set_chunk_size($stream, $size); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
}
--EXPECT--
int(8192)
int(1024)
bool(true)
bool(true)
bool(false)
int(0)
int(-1)
bool(false)
int(8192)
int(1024)
bool(false)
bool(true)
bool(false)
int(0)
int(-1)
bool(false)
int(8192)
int(1024)
bool(false)
bool(true)
bool(false)
int(0)
int(-1)
bool(false)
int(8192)
int(1024)
bool(false)
bool(false)
bool(false)
int(0)
int(-1)
bool(false)
stream_set_chunk_size(): Argument #2 ($size) must be greater than 0
stream_set_chunk_size(): Argument #2 ($size) is too large
