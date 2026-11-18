--TEST--
A seeded engine copy starts at the same position as the source
--FILE--
<?php
$engine = new Random\Engine\Xoshiro256StarStar(1234);
$engine->generate();
$copy = clone $engine;
var_dump($copy->generate() === $engine->generate());
--EXPECT--
bool(true)
