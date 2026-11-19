--TEST--
Callable parameters and results use the declaration scope
--FILE--
<?php
// A `callable` parameter or return type is checked in the scope of the function that declares it, so a private method of that class counts.
class Handler {
    private $stored;
    public function __construct() { $this->stored = [$this, 'hidden']; }
    private function hidden($x) { return "hidden($x)"; }
    protected function guarded($x) { return "guarded($x)"; }
    private static function quiet($x) { return "quiet($x)"; }
    public function set(?callable $handler): ?callable { $previous = $this->stored; $this->stored = $handler; return $previous; }
    public function run($x) { return ($this->stored)($x); }
    public static function check(callable $c) { return 'ok'; }
    public function typedProperty(): string { try { $this->typed = [$this, 'hidden']; return 'assigned'; } catch (TypeError $e) { return $e->getMessage(); } }
    public Closure|array|null $typed = null;
}
class Child extends Handler { public function tryParentPrivate() { return parent::check([$this, 'hidden']); } }
function outside(callable $c) { return 'ok'; }
function attempt(callable $f) { try { echo json_encode($f()), "\n"; } catch (Throwable $e) { echo get_class($e), ': ', preg_replace('~, called in.*~', '', $e->getMessage()), "\n"; } }
$h = new Handler;
attempt(fn () => is_callable($h->set(null)));                      // returns the private array callable out of class scope
attempt(fn () => $h->set([$h, 'hidden']) === null);                // private method, passed from outside: accepted
attempt(fn () => $h->run(1));
attempt(fn () => $h->set([$h, 'guarded']) !== null);
attempt(fn () => $h->set('Handler::quiet') !== null);
attempt(fn () => $h->set([$h, 'missing']));
attempt(fn () => Handler::check([$h, 'hidden']));
attempt(fn () => Handler::check(['Handler', 'quiet']));
attempt(fn () => outside([$h, 'hidden']));                          // a plain function has no class scope: refused
attempt(fn () => (new Child)->tryParentPrivate());
attempt(fn () => is_callable([$h, 'hidden']));
attempt(fn () => (function (callable $c) { return 'bound ok'; })->bindTo($h, Handler::class)([$h, 'hidden']));
attempt(fn () => array_map([$h, 'hidden'], [1]));
attempt(fn () => $h->typedProperty());

--EXPECT--
false
true
"hidden(1)"
true
true
TypeError: Handler::set(): Argument #1 ($handler) must be of type ?callable, array given
"ok"
"ok"
TypeError: outside(): Argument #1 ($c) must be of type callable, array given
"ok"
false
"bound ok"
TypeError: array_map(): Argument #1 ($callback) must be a valid callback or null, cannot access private method Handler::hidden()
"assigned"
