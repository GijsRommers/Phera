--TEST--
Exceptions: finally_typed_override
--FILE--
<?php
function run():int{try{return [];}finally{return 7;}} var_dump(run());
--EXPECTF--
int(7)
