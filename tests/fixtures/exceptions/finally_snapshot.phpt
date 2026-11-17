--TEST--
Exceptions: finally_snapshot
--FILE--
<?php
function run(){ $value=1; try{return $value;}finally{$value=2;}} echo run();
--EXPECTF--
1