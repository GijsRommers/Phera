--TEST--
Integer mode 1 fails before changing the Mt19937 state
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
mt_srand(42);
$expected = mt_rand();
mt_srand(42);
try {
    mt_srand(1, 1);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
try {
    srand(1, 1);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
try {
    new Random\Engine\Mt19937(42, 1);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
var_dump(mt_rand() === $expected);
--EXPECT--
Phera\NotSupportedException: MT_RAND_PHP is not supported by Phera: deprecated since PHP 8.3, as it uses a biased non-standard variant of Mt19937
Phera\NotSupportedException: MT_RAND_PHP is not supported by Phera: deprecated since PHP 8.3, as it uses a biased non-standard variant of Mt19937
Phera\NotSupportedException: MT_RAND_PHP is not supported by Phera: deprecated since PHP 8.3, as it uses a biased non-standard variant of Mt19937
bool(true)
