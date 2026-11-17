--TEST--
Call window: native callback type error
--FILE--
<?php
declare(strict_types=1);try{array_map(fn(int $x)=>$x,[[]]);}catch(Throwable $e){echo get_class($e);}var_dump(array_map(fn(int $x)=>$x,['7']));
--EXPECT--
TypeErrorarray(1) {
  [0]=>
  int(7)
}
