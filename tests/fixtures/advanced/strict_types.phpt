--TEST--
strict_types
--FILE--
<?php
declare(strict_types=1);function f(int $x):int{return $x*2;}echo f(3);
--EXPECT--
6