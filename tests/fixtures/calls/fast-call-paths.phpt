--TEST--
Calls the interpreter loop makes itself behave as the general path: private shadowing, forwarding, closures, types, defaults
--FILE--
<?php
class A { private function f() { return "A::f"; } public function g() { return $this->f(); }
  public static function who() { return static::class; } public static function call() { return self::who() . "/" . static::who(); }
  protected function prot() { return "A::prot"; } public function viaProt() { return $this->prot(); }
  private static function hidden() { return "hidden"; } public static function useHidden() { return self::hidden(); } }
class B extends A { public function f() { return "B::f"; } public function h() { return parent::g() . "+" . $this->f(); }
  protected function prot() { return "B::prot"; } }
for ($i = 0; $i < 3; $i++) { $b = new B; echo $b->g(), " ", $b->h(), " ", B::call(), " ", A::call(), " ", $b->viaProt(), " ", A::useHidden(), "\n"; }
interface I { public function next(I $x): I; }
class L implements I { public function next(I $x): I { return $x; } public function num(int $a, float $b = 1.5): float { return $a + $b; } }
$l = new L; $m = new L; for ($i = 0; $i < 3; $i++) { $l = $l->next($m); echo get_class($l), $l->num(2), $l->num(2, 3), "\n"; }
try { $l->num("x"); } catch (TypeError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { $l->next(5); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$k = 10; $c = function ($v, $w = 2) use ($k) { return $v * $w + $k; };
for ($i = 0; $i < 3; $i++) echo $c($i), " ", $c($i, 5), "\n";
class F { private $o = 3; function make() { return function ($v) { return $this->o + $v; }; } }
$f = (new F)->make(); echo $f(4), "\n";
function d($a, $b = 7) { return func_num_args() . ":" . ($a + $b); } for ($i = 0; $i < 2; $i++) echo d(1), " ", d(1, 2), "\n";
function t(int $x): string { return $x; } echo t(5), gettype(t(5)), "\n";
try { t(); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
class C { function __construct(public int $v = 1) {} function get() { return $this->v; } } echo (new C(4))->get(), (new C)->get(), "\n";
--EXPECTF--
A::f A::f+B::f B/B A/A B::prot hidden
A::f A::f+B::f B/B A/A B::prot hidden
A::f A::f+B::f B/B A/A B::prot hidden
L3.55
L3.55
L3.55
TypeError: L::num(): Argument #1 ($a) must be of type int, string given, called in %s on line 12
L::next(): Argument #1 ($x) must be of type I, int given, called in %s on line 13
10 10
12 15
14 20
7
1:8 2:3
1:8 2:3
5string
Too few arguments to function t(), 0 passed in %s on line 20 and exactly 1 expected
41
