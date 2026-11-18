--TEST--
Static call loads
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
var_dump(AutoloadExample::read());
--EXPECT--
load AutoloadExample
int(8)
