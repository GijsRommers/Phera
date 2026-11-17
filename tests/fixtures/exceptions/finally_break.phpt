--TEST--
Exceptions: finally_break
--FILE--
<?php
for($index=0;$index<5;$index++){try {if($index==3)break;continue;} finally {echo $index;}}
--EXPECTF--
0123