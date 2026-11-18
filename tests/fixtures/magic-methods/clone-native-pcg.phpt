--TEST--
Cloning a seeded PcgOneseq128XslRr64 copies its current state
--FILE--
<?php
$engine = new Random\Engine\PcgOneseq128XslRr64(123);
$engine->generate();
$copy = clone $engine;
var_dump($engine !== $copy, $engine->generate() === $copy->generate());
$copy->generate();
var_dump($engine->generate() !== $copy->generate());
--EXPECT--
bool(true)
bool(true)
bool(true)
