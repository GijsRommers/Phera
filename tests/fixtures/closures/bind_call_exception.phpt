--TEST--
bind call exception
--FILE--
<?php
class Box{}$f=function(){throw new RuntimeException("bound");};try{$f->call(new Box);}catch(RuntimeException $e){echo $e->getMessage(),"
";}
--EXPECT--
bound
