--TEST--
Exceptions: finally_typed_error
--FILE--
<?php
function run():int{try{return [];}catch(TypeError){return 4;}finally{echo "cleanup";}} var_dump(run());
--EXPECTF--
cleanupint(4)
