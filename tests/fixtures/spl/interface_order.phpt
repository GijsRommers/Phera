--TEST--
Native SPL interfaces follow Zend registration order in class listings and reflection
--FILE--
<?php
$object = new ArrayObject;
echo implode(', ', class_implements($object)), "\n";
echo implode(', ', (new ReflectionClass($object))->getInterfaceNames()), "\n";
echo implode(', ', (new ReflectionClass(CachingIterator::class))->getInterfaceNames()), "\n";
?>
--EXPECT--
IteratorAggregate, Traversable, ArrayAccess, Serializable, Countable
IteratorAggregate, Traversable, ArrayAccess, Serializable, Countable
Stringable, Iterator, Traversable, OuterIterator, ArrayAccess, Countable
