--TEST--
short_or
--FILE--
<?php
$x=0;true || ++$x;echo $x;
--EXPECT--
0