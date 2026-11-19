--TEST--
An invalid option quantity warns before looking for the request
--FILE--
<?php
set_error_handler(function (int $level, string $message): bool {
    echo $level, ': ', $message, PHP_EOL;
    return true;
});
try {
    request_parse_body(['upload_max_filesize' => '1GB']);
} catch (RequestParseBodyException $e) {
    echo $e->getMessage(), PHP_EOL;
}
--EXPECT--
2: Invalid quantity "1GB": unknown multiplier "B", interpreting as "1" for backwards compatibility
Request does not provide a content type
