--TEST--
Creating a first-class callable resolves the omitted callee
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    $callback = lcg_value(...);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: lcg_value() is not supported by Phera: deprecated since PHP 8.4, use \Random\Randomizer::getFloat() instead
