--TEST--
Exceptions: finally_break_override
--FILE--
<?php
for($index=0;$index<2;$index++){try{throw new Exception("discard");}finally{break;}}echo "done";
--EXPECTF--

Fatal error: jump out of a finally block is disallowed in %s on line 2
