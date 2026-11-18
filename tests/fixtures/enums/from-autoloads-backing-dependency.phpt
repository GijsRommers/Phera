--TEST--
enums: from autoloads backing dependency
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "loading $name", PHP_EOL;
    class Backing { const X = 42; }
});
enum Loaded: int { case Item = Backing::X; }
var_dump(Loaded::from(42));
--EXPECT--
loading Backing
enum(Loaded::Item)
