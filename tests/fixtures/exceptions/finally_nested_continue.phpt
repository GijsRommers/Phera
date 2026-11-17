--TEST--
Exceptions: finally_nested_continue
--FILE--
<?php
for($outer=0;$outer<2;$outer++){try{for($inner=0;$inner<2;$inner++){try{continue 2;}finally{echo "inner";}}}finally{echo "outer";}}
--EXPECTF--
innerouterinnerouter