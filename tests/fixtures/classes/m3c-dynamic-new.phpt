--TEST--
Class conformance: dynamic-new
--FILE--
<?php class C {} $n='C'; $o=new $n; echo $o instanceof $n ? 'y':'n';
--EXPECT--
y
