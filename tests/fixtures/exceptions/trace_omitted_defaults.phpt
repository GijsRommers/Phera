--TEST--
Exceptions: trace_omitted_defaults
--FILE--
<?php
function create($number=7){return new Exception;}echo create()->getTraceAsString();
--EXPECTF--
#0 %s(2): create()
#1 {main}