--TEST--
anonymous-classes: inherited constructor
--FILE--
<?php
class P { function __construct(public int $x) {} } echo (new class(5) extends P {})->x;
--EXPECT--
5
