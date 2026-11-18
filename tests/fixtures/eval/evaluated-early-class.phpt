--TEST--
Evaluated early class
--FILE--
<?php
var_dump(eval('return class_exists("Early", false); class Early {}'));
--EXPECT--
bool(true)
