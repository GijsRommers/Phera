--TEST--
A supplied null default can use weak scalar coercion
--FILE--
<?php
set_error_handler(static function (int $severity, string $message) {
    echo $message, "\n";
    return true;
});
try {
    mb_encode_mimeheader('hello', indent: 0);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
--EXPECT--
mb_encode_mimeheader(): Passing null to parameter #2 ($charset) of type ?string is deprecated
mb_encode_mimeheader(): Passing null to parameter #3 ($transfer_encoding) of type ?string is deprecated
mb_encode_mimeheader(): Argument #2 ($charset) must be a valid encoding, "" given
