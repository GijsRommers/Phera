--TEST--
Cycle collection follows closure captures, statics and bound receivers
--FILE--
<?php
class CycleOwner {
    public $callback;
    public $copy;

    public function __construct(private string $label) {}
    public function __destruct() { echo "destruct {$this->label}\n"; }
    public function value() { return $this; }
    public function bound() { return function () { return $this; }; }
    public function __call(string $name, array $arguments) { return $this; }
}

function collectOwner(string $label, Closure $make): void {
    $owner = new CycleOwner($label);
    $owner->callback = $make($owner);
    $watch = WeakReference::create($owner);
    unset($owner);
    var_dump(gc_collect_cycles(), $watch->get() === null);
}

collectOwner('capture', static fn($owner) => static fn() => $owner);
collectOwner('bound', static fn($owner) => $owner->bound());
collectOwner('callable', static fn($owner) => $owner->value(...));
collectOwner('static', static function ($owner) {
    $callback = static function ($value) {
        static $saved;
        $saved = $value;
    };
    $callback($owner);
    return $callback;
});
collectOwner('magic', static function ($owner) {
    $closure = $owner->missing(...);
    $owner->copy = clone $closure;
    return $closure;
});
?>
--EXPECT--
destruct capture
int(2)
bool(true)
destruct bound
int(2)
bool(true)
destruct callable
int(2)
bool(true)
destruct static
int(2)
bool(true)
destruct magic
int(3)
bool(true)
