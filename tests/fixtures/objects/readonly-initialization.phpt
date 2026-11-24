--TEST--
Readonly properties initialized in constructors, often enough for the loop to take them, next to every write PHP refuses or converts: a second write, a write from outside or from a child, a conversion, __set after unset, __clone, a lazy ghost, a reference, a readonly class.
--FILE--
<?php
// Readonly properties initialized in constructors, run often enough for the
// loop to take them, next to every write PHP refuses or converts.

class Connection {}
class PdoConnection extends Connection {}

final class Result {
    public function __construct(private readonly Connection $connection, public readonly int $rows, protected readonly ?string $sql = null) {}
    public function sql(): ?string { return $this->sql; }
    public function connection(): Connection { return $this->connection; }
    public function again(): void { $this->rows = 2; }
}

class Base {
    public readonly array $items;
    public function __construct() { $this->items = ['a']; }
}
class Child extends Base {
    public readonly string $name;
    public function __construct() { parent::__construct(); $this->name = 'child'; }
}
class LateChild extends Base {
    public function __construct() { }
    public function fill() { $this->items = ['from child']; }
}
class Floats { public function __construct(public readonly float $f) {} }
class Magic {
    public readonly int $value;
    public function __construct() { unset($this->value); }
    public function __set($name, $v) { echo "__set($name) "; }
    public function init() { $this->value = 5; }
}
class Cloned {
    public function __construct(public readonly array $data) {}
    public function __clone() { $this->data = ['cloned']; }
}
readonly class Point { public function __construct(public int $x, public int $y) {} }

$total = 0;
for ($i = 0; $i < 50; $i++) {
    $r = new Result($i % 2 ? new PdoConnection : new Connection, $i, $i % 3 ? "q$i" : null);
    $total += $r->rows;
    $c = new Child;
    $p = new Point($i, -$i);
    $total += $p->x + $p->y;
}
echo $total, ' ', get_class($r->connection()), ' ', var_export($r->sql(), true), ' ', $c->name, ' ', implode(',', $c->items), "\n";
try { $r->again(); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->rows = 3; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $p->x = 1; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
$late = new LateChild;
try { $late->fill(); echo implode(',', $late->items), "\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $late->fill(); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
for ($i = 0; $i < 3; $i++) { var_dump((new Floats($i))->f); }
try { new Floats('x'); } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { new Result(new stdClass, 1); } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
$m = new Magic;
$m->init();
var_dump(isset($m->value));
$orig = new Cloned(['orig']);
$copy = clone $orig;
echo implode(',', $orig->data), ' ', implode(',', $copy->data), "\n";
$ghost = (new ReflectionClass(Floats::class))->newLazyGhost(function ($o) { echo "init "; $o->__construct(1.5); });
var_dump($ghost->f);
$x = 1;
try { (function () use (&$x) { $this->items = &$x; })->call(new LateChild); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
class Converts { public readonly float $g; public function __construct() { $this->g = 3; } }
for ($i = 0; $i < 3; $i++) { var_dump((new Converts)->g); }
$outside = new LateChild;
try { $outside->items = ['outside']; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }

--EXPECTF--
1225 PdoConnection 'q49' child a
Error: Cannot modify readonly property Result::$rows
Error: Cannot modify readonly property Result::$rows
Error: Cannot modify readonly property Point::$x
from child
Error: Cannot modify readonly property Base::$items
float(0)
float(1)
float(2)
TypeError: Floats::__construct(): Argument #1 ($f) must be of type float, string given, called in %s on line 56
TypeError: Result::__construct(): Argument #1 ($connection) must be of type Connection, stdClass given, called in %s on line 57
__set(value) bool(false)
orig cloned
init float(1.5)
Error: Cannot indirectly modify readonly property Base::$items
float(3)
float(3)
float(3)
Error: Cannot modify protected(set) readonly property Base::$items from global scope
