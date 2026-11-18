--TEST--
Static array property unset accepts a null key
--FILE--
<?php
class Box { public static array $data = ['' => 1]; }
unset(Box::$data[null]);
var_dump(Box::$data);
--EXPECT--
array(0) {
}
