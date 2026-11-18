--TEST--
Computed static call loads
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
$name = "AutoloadExample"; var_dump($name::read());
--EXPECT--
load AutoloadExample
int(8)
