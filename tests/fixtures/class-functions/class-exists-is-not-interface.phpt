--TEST--
Class exists is not interface
--FILE--
<?php
interface Example {} var_dump(class_exists("Example"));
--EXPECT--
bool(false)
