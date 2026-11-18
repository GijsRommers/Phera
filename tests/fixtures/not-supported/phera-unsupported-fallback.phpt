--TEST--
A namespaced lookup falls back only for an unqualified name
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
namespace App;
try {
    strftime('%Y');
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
try {
    var_dump(MT_RAND_PHP);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
try { \App\strftime('%Y'); } catch (\Error $error) { echo $error->getMessage(), "\n"; }
--EXPECT--
Phera\NotSupportedException: strftime() is not supported by Phera: deprecated since PHP 8.1, use IntlDateFormatter::format() instead
Phera\NotSupportedException: MT_RAND_PHP is not supported by Phera: deprecated since PHP 8.3, as it uses a biased non-standard variant of Mt19937
Call to undefined function App\strftime()
