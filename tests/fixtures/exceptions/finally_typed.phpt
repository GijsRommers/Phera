--TEST--
Exceptions: finally_typed
--FILE--
<?php
function run():int{try{return "7";}finally{echo "cleanup";}} var_dump(run());
--EXPECTF--
cleanupint(7)
