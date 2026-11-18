--TEST--
Class exists named argument
--FILE--
<?php
class Example {} var_dump(class_exists(autoload:false, class:"Example"));
--EXPECT--
bool(true)
