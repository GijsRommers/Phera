--TEST--
Static property isset loads
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
var_dump(isset(AutoloadExample::$value));
--EXPECT--
load AutoloadExample
bool(true)
