--TEST--
anonymous-classes: var dump class prefix
--FILE--
<?php
var_dump(new class { private $x=1; });
--EXPECT--
object(class@anonymous)#1 (1) {
  ["x":"class@anonymous":private]=>
  int(1)
}
