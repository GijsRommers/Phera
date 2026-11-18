--TEST--
enums: static method
--FILE--
<?php
enum E{case A;static function initial(){return self::A;}} var_dump(E::initial()===E::A);
--EXPECT--
bool(true)
