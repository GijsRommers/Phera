--TEST--
The CLI has no request content type
--FILE--
<?php
foreach ([[], [null], [[]]] as $args) {
    try {
        request_parse_body(...$args);
    } catch (RequestParseBodyException $e) {
        echo get_class($e), ' extends ', get_parent_class($e), ': ', $e->getMessage(), PHP_EOL;
    }
}
--EXPECT--
RequestParseBodyException extends Exception: Request does not provide a content type
RequestParseBodyException extends Exception: Request does not provide a content type
RequestParseBodyException extends Exception: Request does not provide a content type
