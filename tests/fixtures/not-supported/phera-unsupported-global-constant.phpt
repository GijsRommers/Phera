--TEST--
An omitted global constant is absent and reports its deprecation when read
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    var_dump(MT_RAND_PHP);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: MT_RAND_PHP is not supported by Phera: deprecated since PHP 8.3, as it uses a biased non-standard variant of Mt19937
