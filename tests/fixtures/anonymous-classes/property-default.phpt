--TEST--
anonymous-classes: property default
--FILE--
<?php
echo (new class { public int $answer = 7; })->answer;
--EXPECT--
7
