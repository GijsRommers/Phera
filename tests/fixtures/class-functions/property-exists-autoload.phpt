--TEST--
Property exists autoload
--FILE--
<?php
spl_autoload_register(function($n) { class A { public $p; } }); var_dump(property_exists("A", "p"));
--EXPECT--
bool(true)
