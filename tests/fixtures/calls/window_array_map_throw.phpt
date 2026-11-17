--TEST--
Call window: array map throw
--FILE--
<?php
$x=0;try{array_map(function($v)use(&$x){$x++;if($v==2)throw new Exception('map');return $v;},[1,2,3]);}catch(Throwable $e){echo $e->getMessage();}var_dump($x);var_dump(array_map(fn($v)=>$v+1,[4]));
--EXPECT--
mapint(2)
array(1) {
  [0]=>
  int(5)
}
