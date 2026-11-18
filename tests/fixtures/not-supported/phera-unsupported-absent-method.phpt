--TEST--
An omitted method on an unshipped extension class
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    mysqli::refresh();
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: mysqli::refresh() is not supported by Phera: deprecated since PHP 8.4, use FLUSH SQL statement instead
