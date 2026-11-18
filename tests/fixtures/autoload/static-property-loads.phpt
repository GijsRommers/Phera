--TEST--
Static property loads
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
var_dump(AutoloadExample::$value);
--EXPECT--
load AutoloadExample
int(6)
