--TEST--
Method exists ignores case
--FILE--
<?php
class A { private function run() {} } var_dump(method_exists(A::class, "RUN"));
--EXPECT--
bool(true)
