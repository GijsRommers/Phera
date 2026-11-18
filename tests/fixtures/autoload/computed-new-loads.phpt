--TEST--
Computed new loads
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
$name = "AutoloadExample"; var_dump((new $name)->value());
--EXPECT--
load AutoloadExample
int(7)
