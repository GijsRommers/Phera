--TEST--
warning_int_offset
--FILE--
<?php
$a=3;echo $a['x'];
--EXPECTF--

Warning: Trying to access array offset on int in %s on line 2
