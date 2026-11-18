--TEST--
Cloning a seeded Mt19937 copies its current state
--FILE--
<?php
$engine = new Random\Engine\Mt19937(123);
$engine->generate();
$copy = clone $engine;
var_dump($engine !== $copy, $engine->generate() === $copy->generate());
$copy->generate();
var_dump($engine->generate() !== $copy->generate());
--EXPECT--
bool(true)
bool(true)
bool(true)
