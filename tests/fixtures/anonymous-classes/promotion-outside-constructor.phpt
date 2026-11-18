--TEST--
anonymous-classes: promotion outside constructor
--FILE--
<?php
new class { function f(public int $a) {} };
--EXPECTF--
Fatal error: Cannot declare promoted property outside a constructor in %s on line 2
