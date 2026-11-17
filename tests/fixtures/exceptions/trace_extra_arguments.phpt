--TEST--
Exceptions: trace_extra_arguments
--FILE--
<?php
function create($number){return new Exception;}echo create(1,2,3)->getTraceAsString();
--EXPECTF--
#0 %s(2): create(1, 2, 3)
#1 {main}