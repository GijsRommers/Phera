--TEST--
anonymous-classes: class constant
--FILE--
<?php
echo (new class { const X = 11; })::X;
--EXPECT--
11
