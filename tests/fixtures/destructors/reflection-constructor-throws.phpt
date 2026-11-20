--TEST--
reflection-constructor-throws
--FILE--
<?php
// An object whose constructor throws gets no destructor, whichever way it
// was constructed; a manual __construct call and a failing __clone are
// ordinary method calls, and the object is destructed.
class T {
    public function __construct(public string $n = 'x', bool $fail = true) { if ($fail) throw new Exception("ctor {$this->n}"); }
    public function __clone() { throw new Exception("clone {$this->n}"); }
    public function __destruct() { echo "destruct {$this->n}\n"; }
}
#[Attribute] class A { public function __construct() { throw new Exception("attr"); } public function __destruct() { echo "destruct attribute\n"; } }
#[A] class Marked {}
function attempt(string $what, callable $f) { try { $f(); } catch (Exception $e) { echo "$what: ", $e->getMessage(), "\n"; } }
$r = new ReflectionClass(T::class);
attempt('new', fn () => new T('new'));
attempt('newInstance', fn () => $r->newInstance('newInstance'));
attempt('newInstanceArgs', fn () => $r->newInstanceArgs(['newInstanceArgs']));
attempt('named args', fn () => $r->newInstanceArgs(['n' => 'named']));
attempt('without constructor, then manual __construct', function () use ($r) { $o = $r->newInstanceWithoutConstructor(); $o->n = 'manual'; $o->__construct('manual'); });
attempt('clone', function () { $o = new T('original', false); $c = clone $o; });
attempt('attribute', fn () => (new ReflectionClass(Marked::class))->getAttributes()[0]->newInstance());
attempt('argument throws', fn () => new T((function () { throw new Exception("arg"); })()));
echo "end\n";
--EXPECT--
new: ctor new
newInstance: ctor newInstance
newInstanceArgs: ctor newInstanceArgs
named args: ctor named
destruct manual
without constructor, then manual __construct: ctor manual
destruct original
destruct original
clone: clone original
attribute: attr
argument throws: arg
end
