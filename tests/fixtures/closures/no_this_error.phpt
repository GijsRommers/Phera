--TEST--
no this error
--FILE--
<?php
$f=function(){return $this;};try{$f();}catch(Error $e){echo $e->getMessage(),"
";}
--EXPECT--
Using $this when not in object context
