--TEST--
Exceptions: finally_double_return
--FILE--
<?php
function run(){try{return 1;}finally{return 2;}} echo run();
--EXPECTF--
2