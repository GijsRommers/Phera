--TEST--
Callable array loads class
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
$f = ["AutoloadExample", "read"]; var_dump($f());
--EXPECT--
load AutoloadExample
int(8)
