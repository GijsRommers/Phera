--TEST--
enums: pure enum can declare from
--FILE--
<?php
enum E{static function from($value){return $value;}} echo E::from("ok");
--EXPECT--
ok
