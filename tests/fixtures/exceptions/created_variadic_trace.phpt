--TEST--
Exceptions: created_variadic_trace
--FILE--
<?php
function make(...$numbers){return new Exception("x");} $error=make(first:5.0,second:"x\ny");echo $error->getTraceAsString();
--EXPECTF--
#0 %s(2): make(first: 5.0, second: 'x\ny')
#1 {main}