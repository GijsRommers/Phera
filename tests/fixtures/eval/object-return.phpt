--TEST--
Object return
--FILE--
<?php
var_dump(get_class(eval('return new stdClass;')));
--EXPECT--
string(8) "stdClass"
