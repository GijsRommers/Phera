--TEST--
closure exception
--FILE--
<?php
$f=function(){try{throw new RuntimeException("callback");}finally{echo "finally
";}};try{$f();}catch(RuntimeException $e){echo $e->getMessage(),"
";}
--EXPECT--
finally
callback
