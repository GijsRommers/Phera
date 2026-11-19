--TEST--
A temporary copy of an object does not outlive its statement (object ids)
--FILE--
<?php
class Item {
    public function self(): static { return $this; }
    public function __toString(): string { return 'item'; }
}
function same(object $o): object { return $o; }
$y = new Item;
$probe = new stdClass;
$freed = spl_object_id($probe);
unset($probe);
// An object freed when its statement ends gives its id to the next one.
$x = new Item;
$r = $x === $y;
unset($x);
echo "=== left operand: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = $y === $x;
unset($x);
echo "=== right operand: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = $y == $x;
unset($x);
echo "== right operand: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = $x instanceof Item;
unset($x);
echo "instanceof: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = (null ?? $x) === $y;
unset($x);
echo "?? right operand: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = (0 ?: $x) === $y;
unset($x);
echo "?: right operand: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = $x ? 1 : 2;
unset($x);
echo "ternary condition: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = 'a' . $x . 'b';
unset($x);
echo ". through __toString: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = count([$x]);
unset($x);
echo "argument: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = max($y === $x, $y == $x);
unset($x);
echo "comparison inside arguments: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = $y === [$x][0];
unset($x);
echo "array element in a chain: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = $y === same($x);
unset($x);
echo "call result operand: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$x->self()->self();
unset($x);
echo "discarded method chain: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
$x = new Item;
$r = $y === $x || $y === $x || $y == $x;
unset($x);
echo "chain of operands: ", spl_object_id(new stdClass) === $freed ? "freed" : "alive", "\n";
--EXPECT--
=== left operand: freed
=== right operand: freed
== right operand: freed
instanceof: freed
?? right operand: freed
?: right operand: freed
ternary condition: freed
. through __toString: freed
argument: freed
comparison inside arguments: freed
array element in a chain: freed
call result operand: freed
discarded method chain: freed
chain of operands: freed
