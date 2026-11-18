--TEST--
Instanceof does not load
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
var_dump(new stdClass instanceof AutoloadExample);
--EXPECT--
bool(false)
