--TEST--
anonymous-classes: abstract method
--FILE--
<?php
new class { abstract function x(); };
--EXPECTF--
Fatal error: Anonymous class method x() must not be abstract in /private%s on line 2
