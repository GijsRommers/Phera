--TEST--
Type check does not load
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
function typed(AutoloadExample $value) {} try { typed(new stdClass); } catch (TypeError $e) { echo "type error\n"; }
--EXPECT--
type error
