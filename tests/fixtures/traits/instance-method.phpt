--TEST--
traits: instance method
--FILE--
<?php
trait T { function one(){return 1;} } class C { use T; } echo (new C)->one();
--EXPECT--
1
