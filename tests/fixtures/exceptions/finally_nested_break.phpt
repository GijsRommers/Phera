--TEST--
Exceptions: finally_nested_break
--FILE--
<?php
for($outer=0;$outer<3;$outer++){try{for($inner=0;$inner<3;$inner++){try{break 2;}finally{echo "inner";}}}finally{echo "outer";}} echo "done";
--EXPECTF--
innerouterdone