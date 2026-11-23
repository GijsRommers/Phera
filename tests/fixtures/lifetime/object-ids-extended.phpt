--TEST--
object-ids-extended
--FILE--
<?php
class A { public function hi() { return 1; } public static function make(): static { return new static; } public function self() { return $this; } }
function id() { return spl_object_id(new A); }
echo "start ", id(), "\n";
foreach ([fn() => 1, fn() => 2, fn() => 3] as $f) { $f(); }
echo "after foreach over closures ", id(), "\n";
try { (new A)->nope(); } catch (Error $e) {} unset($e);
echo "after failed call on temporary ", id(), "\n";
var_dump(get_class(A::make()), get_class(A::make()));
echo "after temporaries as arguments ", id(), "\n";
function f(A $a) { return 1; } f(new A); f(new A);
echo "after passing temporaries ", id(), "\n";
$c = function () { return new A; }; $c(); $c();
echo "after closure results ", id(), "\n";
unset($f, $c);
echo "after unset closures ", id(), "\n";
$s = "x" . spl_object_id(new A) . "{$c2}";
echo "interp ", id(), "\n";
$arr = [new A, new A]; $arr = null;
echo "array literal ", id(), "\n";
$t = true ? new A : null; $t = null;
echo "ternary ", id(), "\n";
$n = null ?? new A; unset($n);
echo "coalesce ", id(), "\n";
$m = match(1) { 1 => new A, default => null }; $m = 1;
echo "match ", id(), "\n";
[$p, $q] = [new A, new A]; unset($p); $q = 2;
echo "list ", id(), "\n";
(new A)->self()->self()->hi();
echo "chain ", id(), "\n";
$g = function () { return $this; }; $b = Closure::bind($g, new A, A::class); unset($b, $g);
echo "bound this ", id(), "\n";
$o = new A; $h = function () use ($o) { return 1; }; unset($o); $h(); unset($h);
echo "captured ", id(), "\n";
function thrower() { $local = new A; throw new Exception("x"); }
try { thrower(); } catch (Exception $e) { } unset($e);
echo "unwound ", id(), "\n";
for ($i = 0; $i < 3; $i++) { $tmp = new A; } unset($tmp);
echo "loop ", id(), "\n";
echo strlen(get_class(new A)), " ", id(), "\n";
$x = new A; $x = new A; unset($x);
echo "reassign ", id(), "\n";
function r() { return new A; } r()->hi();
echo "call result method ", id(), "\n";
echo (new A)->hi() + (new A)->hi(), " ", id(), "\n";
--EXPECTF--
start 1
after foreach over closures 2
after failed call on temporary 1
string(1) "A"
string(1) "A"
after temporaries as arguments 1
after passing temporaries 1
after closure results 2
after unset closures 1

Warning: Undefined variable $c2 in %s on line 17
interp 1
array literal 3
ternary 3
coalesce 3
match 3
list 1
chain 1
bound this 1
captured 2
unwound 1
loop 1
1 1
reassign 2
call result method 2
2 2
