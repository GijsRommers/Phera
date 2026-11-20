--TEST--
Native named defaults: native enum default
--FILE--
<?php
$parameter = new ReflectionParameter([Random\Randomizer::class, 'getFloat'], 'boundary');
var_dump($parameter->getDefaultValue() === Random\IntervalBoundary::ClosedOpen);
var_dump($parameter->getDefaultValueConstantName());
$automatic = new Random\Randomizer(new Random\Engine\Mt19937(42));
$explicit = new Random\Randomizer(new Random\Engine\Mt19937(42));
var_dump($automatic->getFloat(-17, 20) === $explicit->getFloat(-17, 20, $parameter->getDefaultValue()));
--EXPECT--
bool(true)
string(35) "Random\IntervalBoundary::ClosedOpen"
bool(true)
