--TEST--
Call user func loads class
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
var_dump(call_user_func("AutoloadExample::read"));
--EXPECT--
load AutoloadExample
int(8)
