--TEST--
An omitted function reports the PHP attribute message
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    STRFTIME('%Y');
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: strftime() is not supported by Phera: deprecated since PHP 8.1, use IntlDateFormatter::format() instead
