--TEST--
Invoke visibility warnings precede execution of untaken class declarations
--FILE--
<?php
if (false) { class A { protected function __invoke() {} } }
echo "done\n";
--EXPECTF--
Warning: The magic method A::__invoke() must have public visibility in %s on line 2
done
