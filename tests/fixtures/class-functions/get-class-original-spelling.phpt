--TEST--
Get class original spelling
--FILE--
<?php
class SomeClass {} var_dump(get_class(new SomeClass));
--EXPECT--
string(9) "SomeClass"
