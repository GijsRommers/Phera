--TEST--
resolve undefined method
--FILE--
<?php
class A{}$f=[new A,"missing"];try{$f();}catch(Error $e){echo $e->getMessage(),"
";}
--EXPECT--
Call to undefined method A::missing()
