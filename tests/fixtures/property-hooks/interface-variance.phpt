--TEST--
A plain property covariantly implements an interface getter
--FILE--
<?php
class Animal {}
class Cat extends Animal {}
interface Owner { public Animal $pet { get; } }
class Person implements Owner {
    public function __construct(public Cat $pet) {}
}
$p = new Person(new Cat);
echo get_class($p->pet), "\n";
--EXPECT--
Cat
