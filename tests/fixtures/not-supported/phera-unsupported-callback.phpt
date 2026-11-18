--TEST--
Callback validation preserves the unsupported exception
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    array_map('strftime', []);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
try {
    call_user_func('lcg_value');
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
try {
    call_user_func_array('Redis::sortAsc', []);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: strftime() is not supported by Phera: deprecated since PHP 8.1, use IntlDateFormatter::format() instead
Phera\NotSupportedException: lcg_value() is not supported by Phera: deprecated since PHP 8.4, use \Random\Randomizer::getFloat() instead
Phera\NotSupportedException: Redis::sortAsc() is not supported by Phera: deprecated by phpredis
