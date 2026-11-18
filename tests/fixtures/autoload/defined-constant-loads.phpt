--TEST--
Defined constant loads
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
var_dump(defined("AutoloadExample::VALUE"));
--EXPECT--
load AutoloadExample
bool(true)
