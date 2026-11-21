--TEST--
Strict types cannot appear after a namespace declaration
--FILE--
<?php
namespace Demo;
declare(strict_types=1);
--EXPECTF--
Fatal error: strict_types declaration must be the very first statement in the script in %s on line 3
