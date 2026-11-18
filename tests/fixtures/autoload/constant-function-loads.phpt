--TEST--
Constant function loads
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
var_dump(constant("AutoloadExample::VALUE"));
--EXPECT--
load AutoloadExample
int(5)
