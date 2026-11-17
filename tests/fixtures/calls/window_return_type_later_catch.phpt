--TEST--
Call window: return type later catch
--FILE--
<?php
function f():int{return [];}try{f();}catch(Throwable $e){echo $e->getMessage();}function g(){return 'ok';}echo g();
--EXPECT--
f(): Return value must be of type int, array returnedok
