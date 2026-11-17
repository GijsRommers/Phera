--TEST--
Exceptions: callback_trace_extras
--FILE--
<?php
function handler(){throw new Exception("warning");}set_error_handler("handler");try{echo $missing;}catch(Throwable $error){echo $error->getTraceAsString();}
--EXPECTF--
#0 %s(2): handler(2, 'Undefined varia...', '%s...', 2)
#1 {main}