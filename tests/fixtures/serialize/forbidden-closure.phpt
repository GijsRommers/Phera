--TEST--
Closures refuse both serialization and unserialization
--FILE--
<?php
foreach ([fn() => serialize(fn() => 1), fn() => unserialize('O:7:"Closure":0:{}')] as $test) {
    try { $test(); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
}
?>
--EXPECT--
Serialization of 'Closure' is not allowed
Unserialization of 'Closure' is not allowed
