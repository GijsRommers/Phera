--TEST--
Class name does not load
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
var_dump(AutoloadExample::class);
--EXPECT--
string(15) "AutoloadExample"
