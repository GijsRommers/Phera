--TEST--
Reflection functions preserve named defaults, return types and callable scope
--FILE--
<?php
const LIMIT = 7;

/** Scales an integer. */
function scale(int $value, int $by = LIMIT): int {
    return $value * $by;
}

$reflect = new ReflectionFunction('scale');
echo $reflect->getDocComment(), "\n";
echo $reflect->getReturnType(), "\n";
$factor = new ReflectionParameter('scale', 'by');
echo $factor->name, " ", $factor->getPosition(), " ", $factor->getDefaultValueConstantName(), " ", $factor->getDefaultValue(), "\n";
echo $reflect->invokeArgs(['by' => 3, 'value' => 5]), "\n";
echo $reflect->getClosure()(5), "\n";
$factor = 9;
$func = new ReflectionFunction(static fn (int $x): int => $x + $factor);
var_dump($func->isStatic(), $func->isAnonymous(), $func->getClosureUsedVariables(), $func->invoke(2));
$native = new ReflectionFunction('strlen');
echo $native->getParameters()[0]->getName(), " ", $native->getParameters()[0]->getType(), " ", $native->invoke('hello'), "\n";
--EXPECT--
/** Scales an integer. */
int
by 1 LIMIT 7
15
35
bool(true)
bool(true)
array(1) {
  ["factor"]=>
  int(9)
}
int(11)
string string 5
