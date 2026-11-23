--TEST--
A namespaced lookup falls back only for an unqualified name
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
namespace App;
use function date_sunset as format_date;
try {
    date_sunset(0);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
try {
    var_dump(MT_RAND_PHP);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
try { \App\date_sunset(0); } catch (\Error $error) { echo $error->getMessage(), "\n"; }
try { format_date(0); } catch (\Phera\NotSupportedException $error) { echo $error->getMessage(), "\n"; }
--EXPECT--
Phera\NotSupportedException: date_sunset() is not supported by Phera: deprecated since PHP 8.1, use date_sun_info() instead
Phera\NotSupportedException: MT_RAND_PHP is not supported by Phera: deprecated since PHP 8.3, as it uses a biased non-standard variant of Mt19937
Call to undefined function App\date_sunset()
date_sunset() is not supported by Phera: deprecated since PHP 8.1, use date_sun_info() instead
