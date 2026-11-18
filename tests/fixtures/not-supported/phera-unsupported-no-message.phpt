--TEST--
An omitted function without an attribute message
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    mhash_count();
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: mhash_count() is not supported by Phera: deprecated since PHP 8.1
