--TEST--
An omitted function reports the PHP attribute message
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    DATE_SUNSET(0);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: date_sunset() is not supported by Phera: deprecated since PHP 8.1, use date_sun_info() instead
