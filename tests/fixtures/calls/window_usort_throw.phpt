--TEST--
Call window: usort throw
--FILE--
<?php
$a=[3,1,2];try{usort($a,function($a,$b){throw new Exception('sort');});}catch(Throwable $e){echo $e->getMessage();}$a=[2,1];usort($a,fn($a,$b)=>$a<=>$b);var_dump($a);
--EXPECT--
sortarray(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
