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
8192: Method ArrayIterator::getFlags() is deprecated since 8.6
int(0)
8192: Method ArrayIterator::setFlags() is deprecated since 8.6
8192: Method ArrayIterator::asort() is deprecated since 8.6
8192: Method ArrayIterator::ksort() is deprecated since 8.6
8192: Method ArrayIterator::natsort() is deprecated since 8.6
8192: Method ArrayIterator::natcasesort() is deprecated since 8.6
bool(true)
bool(true)
bool(true)
bool(true)
8192: Method ArrayIterator::uasort() is deprecated since 8.6
8192: Method ArrayIterator::uksort() is deprecated since 8.6
bool(true)
bool(true)
8192: Method ArrayIterator::serialize() is deprecated since 8.6
8192: Method ArrayIterator::unserialize() is deprecated since 8.6
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(1)
}
bool(true)
bool(true)
int(0)
