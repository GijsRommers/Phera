--TEST--
$this->p in a method whose class fixes the slot (a private property of the class, or a final class), run often enough for the loop to take it, next to everything that must still go the long way.
--FILE--
<?php
// $this->p in a method whose class fixes the slot (a private property of the
// class, or a final class), run often enough for the loop to take it, next
// to everything that must still go the long way.
class Base {
    private $secret = 'base secret';
    private int $count = 0;
    public function secret() { return $this->secret; }
    public function bump() { $this->count = $this->count + 1; return $this->count; }
    public function setCount($v) { try { $this->count = $v; return $this->count; } catch (TypeError $e) { return $e->getMessage(); } }
    public function forget() { unset($this->secret); }
    public function share(&$outside) { $this->secret = &$outside; }
}
class Child extends Base {
    public $secret = 'child public';   // another property: Base's code keeps its own
}
final class Leaf {
    public $open = 'open';
    protected $guarded = 'guarded';
    public readonly int $fixed;
    public static $shared = 'static';
    public function __construct() { $this->fixed = 1; }
    public function read() { return [$this->open, $this->guarded, $this->fixed]; }
    public function write($v) { $this->open = $v; $this->guarded = $v; try { $this->fixed = 2; } catch (Error $e) { return $e->getMessage(); } }
}
class Magic {
    private $hidden = 'there';
    public function __get($n) { return "magic $n"; }
    public function read() { return $this->hidden; }
    public function drop() { unset($this->hidden); }
}
for ($round = 0; $round < 3; $round++) {
    $b = new Base; $c = new Child;
    echo $b->secret(), ' | ', $c->secret(), ' | ', $c->secret, ' | ', $b->bump(), $b->bump(), ' ', $c->bump(), "\n";
    echo $b->setCount(5), ' ', $b->setCount('7'), ' ', $b->setCount(1.5), ' ', $b->setCount('x'), "\n";
    $l = new Leaf;
    echo json_encode($l->read()), ' ', $l->write('new'), ' ', json_encode($l->read()), "\n";
    $m = new Magic; echo $m->read(), ' '; $m->drop(); echo $m->read(), "\n";
    $s = new Base; $outer = 'shared'; $s->share($outer); $outer = 'changed'; echo $s->secret(), "\n";
    $f = new Base; $f->forget(); try { echo $f->secret(), "\n"; } catch (Error $e) { echo get_class($e), "\n"; }
    $lazy = (new ReflectionClass(Base::class))->newLazyGhost(function ($o) { echo "[init]"; });
    echo $lazy->secret(), "\n";
    $bound = Closure::bind(function () { return $this->secret; }, $c, Base::class);
    echo $bound(), "\n";
}
--EXPECTF--
base secret | base secret | child public | 12 1
5 7 
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 10
1 Cannot assign string to property Base::$count of type int
["open","guarded",1] Cannot modify readonly property Leaf::$fixed ["new","new",1]
there magic hidden
changed

Warning: Undefined property: Base::$secret in %s on line 8

[init]base secret
base secret
base secret | base secret | child public | 12 1
5 7 
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 10
1 Cannot assign string to property Base::$count of type int
["open","guarded",1] Cannot modify readonly property Leaf::$fixed ["new","new",1]
there magic hidden
changed

Warning: Undefined property: Base::$secret in %s on line 8

[init]base secret
base secret
base secret | base secret | child public | 12 1
5 7 
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 10
1 Cannot assign string to property Base::$count of type int
["open","guarded",1] Cannot modify readonly property Leaf::$fixed ["new","new",1]
there magic hidden
changed

Warning: Undefined property: Base::$secret in %s on line 8

[init]base secret
base secret
