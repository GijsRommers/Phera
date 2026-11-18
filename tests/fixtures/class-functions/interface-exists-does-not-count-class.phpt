--TEST--
Interface exists does not count class
--FILE--
<?php
class Example {} var_dump(interface_exists("Example"));
--EXPECT--
bool(false)
