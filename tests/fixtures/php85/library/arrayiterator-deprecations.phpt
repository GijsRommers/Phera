--TEST--
ArrayIterator retains deprecated methods and ArrayObject keeps its methods
--FILE--
<?php
set_error_handler(function ($severity, $message) { echo $severity, ': ', $message, PHP_EOL; });
$iterator = new RecursiveArrayIterator([2, 1]);
var_dump($iterator->getFlags());
$iterator->setFlags(0);
$compare = fn($a, $b) => $a <=> $b;
var_dump($iterator->asort(), $iterator->ksort(), $iterator->natsort(), $iterator->natcasesort());
var_dump($iterator->uasort($compare), $iterator->uksort($compare));
$iterator->unserialize($iterator->serialize());
var_dump($iterator->getArrayCopy());
var_dump((new ReflectionMethod($iterator, 'serialize'))->isDeprecated());
var_dump((new ReflectionMethod(SplTempFileObject::class, 'getCsvControl'))->isDeprecated());
var_dump((new ArrayObject)->getFlags());
--EXPECT--
int(0)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(1)
}
bool(false)
bool(false)
int(0)
