--TEST--
class_implements() lists named interfaces before the ones they extend
--FILE--
<?php
interface A {} interface B extends A {} interface C {} interface D extends C, B {}
interface E extends D, A {}
class Bag implements IteratorAggregate, Countable {
    public function getIterator(): Iterator { return $this; }
    public function count(): int { return 0; }
}
class K implements B, C {}
class L extends K implements Countable { public function count(): int { return 0; } }
class M implements D {}
class O extends M implements E, ArrayAccess {
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetSet(mixed $o, mixed $v): void {}
    public function offsetUnset(mixed $o): void {}
}
interface MyList extends IteratorAggregate, Countable, ArrayAccess {}
abstract class It implements Iterator, Countable {}
abstract class N extends It implements ArrayAccess, A {}
foreach (['Bag', 'K', 'L', 'M', 'O', 'MyList', 'It', 'N', 'D', 'E', 'B', 'A'] as $name) {
    echo $name, ': ', implode(',', class_implements($name)), "\n";
}
var_dump(class_implements(new L), class_implements('stdClass'));
--EXPECT--
Bag: IteratorAggregate,Countable,Traversable
K: B,C,A
L: B,C,A,Countable
M: D,A,B,C
O: D,A,B,C,E,ArrayAccess
MyList: IteratorAggregate,Countable,ArrayAccess,Traversable
It: Iterator,Countable,Traversable
N: Iterator,Countable,Traversable,ArrayAccess,A
D: C,B,A
E: D,A,B,C
B: A
A: 
array(4) {
  ["B"]=>
  string(1) "B"
  ["C"]=>
  string(1) "C"
  ["A"]=>
  string(1) "A"
  ["Countable"]=>
  string(9) "Countable"
}
array(0) {
}
