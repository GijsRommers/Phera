--TEST--
Exceptions: finally_return_three_nested
--FILE--
<?php
function run(){try{try{try{return 3;}finally{echo "a";}}finally{echo "b";}}finally{echo "c";}}echo run();
--EXPECTF--
abc3