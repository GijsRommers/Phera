--TEST--
Exceptions: finally_inside_loop
--FILE--
<?php
try{for($index=0;$index<3;$index++){try{continue;}finally{echo $index;}}}finally{echo "outer";}
--EXPECTF--
012outer