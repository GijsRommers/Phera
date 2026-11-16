--TEST--
coalesce_assign
--FILE--
<?php
$a??=3;$a??=4;echo $a;
--EXPECT--
3