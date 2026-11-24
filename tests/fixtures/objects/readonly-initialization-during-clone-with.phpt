--TEST--
Readonly properties initialized while a clone's clone-with updates run, often enough for the loop to take the writes: a hook initializes one, and a later update of the same property in that clone must fail.
--FILE--
<?php
// Readonly properties initialized while a clone's `clone with` updates run,
// often enough for the loop to take the writes: a hook initializes one, and
// a later update of the same property in that clone must fail.

class Clazz {
    public string $foo {
        set {
            $this->foo = $value;
            $this->bar = 'bar updated in hook';
        }
    }
    public public(set) readonly string $bar;
}

final class Point {
    public function __construct(public readonly int $x = 0, public readonly int $y = 0) {}
    public function withX(int $x): static { return clone($this, ['x' => $x]); }
}

$f = new Clazz();
for ($i = 0; $i < 5; $i++) {
    $c = clone($f, ['foo' => "foo $i"]);
    echo $c->foo, ' / ', $c->bar, "\n";
    try {
        clone($f, ['foo' => "foo $i", 'bar' => 'bar updated in clone-with']);
        echo "no error\n";
    } catch (Error $e) {
        echo get_class($e), ': ', $e->getMessage(), "\n";
    }
}
$p = new Point(1, 2);
for ($i = 0; $i < 5; $i++) { $p = $p->withX($i); }
echo $p->x, ',', $p->y, "\n";
try { clone($p, ['x' => 1, 'x' => 2]); echo "ok\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }

--EXPECTF--
foo 0 / bar updated in hook
Error: Cannot modify readonly property Clazz::$bar
foo 1 / bar updated in hook
Error: Cannot modify readonly property Clazz::$bar
foo 2 / bar updated in hook
Error: Cannot modify readonly property Clazz::$bar
foo 3 / bar updated in hook
Error: Cannot modify readonly property Clazz::$bar
foo 4 / bar updated in hook
Error: Cannot modify readonly property Clazz::$bar
4,2
Cannot modify protected(set) readonly property Point::$x from global scope
