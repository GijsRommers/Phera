--TEST--
Catch does not load
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
try { try { throw new Exception; } catch (AutoloadExample $e) {} } catch (Exception $e) { echo "outer\n"; }
--EXPECT--
outer
