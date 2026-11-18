--TEST--
A string constant read reports the omitted name
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    constant('FILTER_SANITIZE_STRING');
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: FILTER_SANITIZE_STRING is not supported by Phera: deprecated since PHP 8.1, use htmlspecialchars() instead
