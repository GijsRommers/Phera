--TEST--
callback strict user func array
--FILE--
<?php
declare(strict_types=1);$f=fn(int $x)=>$x;try{call_user_func_array($f,["3"]);}catch(TypeError $e){var_dump(strpos($e->getMessage(),", called in ")!==false);}
--EXPECT--
bool(true)
