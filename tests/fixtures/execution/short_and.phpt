--TEST--
short_and
--FILE--
<?php
$x=0;false && ++$x;echo $x;
--EXPECT--
0