--TEST--
warning_key
--FILE--
<?php
$a=[];echo $a['missing'];
--EXPECTF--

Warning: Undefined array key "missing" in %s on line 2
