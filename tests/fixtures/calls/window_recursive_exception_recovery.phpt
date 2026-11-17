--TEST--
Call window: recursive exception recovery
--FILE--
<?php
function f($n){if($n==0)throw new Exception('depth');return f($n-1);}try{f(100000);}catch(Throwable $e){echo $e->getMessage();}function g($x){return $x;}echo g('ok');
--EXPECT--
depthok
