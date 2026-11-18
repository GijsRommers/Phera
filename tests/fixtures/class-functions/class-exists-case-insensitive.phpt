--TEST--
Class exists case insensitive
--FILE--
<?php
class Example {} var_dump(class_exists("example"));
--EXPECT--
bool(true)
