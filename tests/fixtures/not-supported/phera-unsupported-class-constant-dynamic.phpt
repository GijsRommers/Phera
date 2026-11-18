--TEST--
Dynamic lookup identifies an omitted class constant
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
$class = 'ZipArchive';
$name = 'FL_RECOMPRESS';
try {
    var_dump($class::{$name});
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: ZipArchive::FL_RECOMPRESS is not supported by Phera: deprecated since PHP 8.3
