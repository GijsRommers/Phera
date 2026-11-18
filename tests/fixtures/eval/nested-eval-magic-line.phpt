--TEST--
Nested eval magic line
--FILE--
<?php
echo eval('return eval("return __LINE__;");');
--EXPECT--
1