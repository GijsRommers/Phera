--TEST--
Class callable lookup autoloads before testing for __callStatic
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load $name\n";
    eval('class MagicAutoload {
        static function __callStatic($n, $args) { echo $n, ":", $args[0]; }
    }');
});
$cb = "MagicAutoload::missing";
var_dump(is_callable($cb), method_exists("MagicAutoload", "missing"));
$cb(8);
--EXPECT--
load MagicAutoload
bool(true)
bool(false)
missing:8
