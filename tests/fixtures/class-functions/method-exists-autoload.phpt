--TEST--
Method exists autoload
--FILE--
<?php
spl_autoload_register(function($n) { class A { public function run() {} } }); var_dump(method_exists("A", "run"));
--EXPECT--
bool(true)
