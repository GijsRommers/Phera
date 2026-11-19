--TEST--
A temporary copy of an object does not outlive its statement (weak references)
--FILE--
<?php
class Item {
    public function self(): static { return $this; }
    public function __toString(): string { return 'item'; }
}
function same(object $o): object { return $o; }
$y = new Item;
$x = new Item;
$w = WeakReference::create($x);
$r = $x === $y;
unset($x);
echo "=== left operand: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = $y === $x;
unset($x);
echo "=== right operand: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = $y == $x;
unset($x);
echo "== right operand: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = $x instanceof Item;
unset($x);
echo "instanceof: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = (null ?? $x) === $y;
unset($x);
echo "?? right operand: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = (0 ?: $x) === $y;
unset($x);
echo "?: right operand: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = $x ? 1 : 2;
unset($x);
echo "ternary condition: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = 'a' . $x . 'b';
unset($x);
echo ". through __toString: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = count([$x]);
unset($x);
echo "argument: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = max($y === $x, $y == $x);
unset($x);
echo "comparison inside arguments: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = $y === [$x][0];
unset($x);
echo "array element in a chain: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = $y === same($x);
unset($x);
echo "call result operand: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$x->self()->self();
unset($x);
echo "discarded method chain: ", $w->get() === null ? "freed" : "alive", "\n";
$x = new Item;
$w = WeakReference::create($x);
$r = $y === $x || $y === $x || $y == $x;
unset($x);
echo "chain of operands: ", $w->get() === null ? "freed" : "alive", "\n";
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
