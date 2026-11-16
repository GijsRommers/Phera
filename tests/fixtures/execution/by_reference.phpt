--TEST--
by_reference
--FILE--
<?php
function f(&$x){$x+=2;}$a=1;f($a);echo $a;
--EXPECT--
3