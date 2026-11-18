--TEST--
Expression new loads
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
var_dump((new AutoloadExample)->value());
--EXPECT--
load AutoloadExample
int(7)
