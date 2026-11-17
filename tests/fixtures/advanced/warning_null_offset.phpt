--TEST--
warning_null_offset
--FILE--
<?php
$a=null;echo $a['x'];
--EXPECTF--

Warning: Trying to access array offset on null in %s on line 2
