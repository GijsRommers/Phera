--TEST--
Destructors run at the releasing statement for every shape the loop runs, while a long-lived object with a destructor exists: overwrite, unset, element and property writes, compound assignment, foreach over a temporary, returns, resurrection, nested order, a generator with finally, catch
--FILE--
<?php
// When a destructor runs, relative to the statements around the release.
class D {
    public static array $kept = [];
    public function __construct(public string $name, public bool $keep = false, public ?D $inner = null) {}
    public function __toString(): string { return $this->name; }
    public function __destruct() {
        echo "  ~{$this->name}\n";
        if ($this->keep) { self::$kept[] = $this; }
    }
}
class Holder { public $p; public array $items = []; }
// A long-lived object with a destructor, as in a framework request.
$service = new D('service');

function overwrite() { $a = new D('overwrite'); echo "before\n"; $a = 1; echo "after\n"; }
function unsetIt() { $a = new D('unset'); echo "before\n"; unset($a); echo "after\n"; }
function element() { $arr = ['k' => new D('element')]; echo "before\n"; $arr['k'] = 2; echo "after\n"; }
function property() { $h = new Holder; $h->p = new D('property'); echo "before\n"; $h->p = null; echo "after\n"; }
function propertyElement() { $h = new Holder; $h->items['x'] = new D('prop-elem'); echo "before\n"; $h->items['x'] = 0; echo "after\n"; }
function compound() { $arr = ['n' => new D('compound')]; echo "before\n"; $arr['n'] .= 'x'; echo "after\n"; }
function make() { return [new D('temp-1'), new D('temp-2')]; }
function foreachTemp() { echo "before\n"; foreach (make() as $i => $d) { echo "  item $i\n"; } echo "loop done\n"; unset($d); echo "after\n"; }
function locals() { $x = new D('local-x'); $y = new D('local-y'); echo "returning\n"; return 5; }
function returnsLast() { $x = new D('returned-away'); return $x->name; }
function keyedForeach() { $list = ['a' => new D('keyed-a'), 'b' => new D('keyed-b')]; foreach ($list as $k => $v) { echo "  at $k\n"; } echo "after loop\n"; $list = null; echo "after clear\n"; }
function intoArrayIndex() { $arr = [new D('index-0')]; $x = new D('held'); echo "before\n"; $x = $arr[0]; echo "after\n"; }

foreach (['overwrite', 'unsetIt', 'element', 'property', 'propertyElement', 'compound', 'foreachTemp', 'keyedForeach', 'intoArrayIndex'] as $f) {
    echo "== $f\n"; $f(); echo "back\n";
}
echo "== locals\n"; echo locals(), "\n"; echo "back\n";
echo "== returnsLast\n"; echo returnsLast(), "\n";

echo "== resurrection\n";
function resurrect() { $r = new D('lazarus', true); $r = null; echo "after release\n"; }
resurrect(); echo "kept: ", count(D::$kept), "\n"; D::$kept = []; echo "released again\n";

echo "== nested order\n";
function nested() { $outer = new D('outer', false, new D('inner')); echo "before\n"; $outer = 0; echo "after\n"; }
nested();

echo "== generator\n";
function gen() { try { yield 1; yield 2; } finally { echo "  finally\n"; } }
function genUse() { $g = gen(); foreach ($g as $v) { echo "  got $v\n"; break; } echo "before\n"; $g = null; echo "after\n"; }
genUse();

echo "== catch with a destructor pending\n";
function thrower() { $t = new D('in-thrower'); throw new RuntimeException('boom'); }
try { thrower(); } catch (RuntimeException $e) { echo "caught ", $e instanceof RuntimeException ? 'yes' : 'no', "\n"; }
try { thrower(); } catch (LogicException $e) { echo "wrong\n"; } catch (Exception $e) { echo "caught as Exception\n"; }

echo "== end\n";

--EXPECTF--
== overwrite
before
  ~overwrite
after
back
== unsetIt
before
  ~unset
after
back
== element
before
  ~element
after
back
== property
before
  ~property
after
back
== propertyElement
before
  ~prop-elem
after
back
== compound
before
  ~compound
after
back
== foreachTemp
before
  item 0
  item 1
  ~temp-1
loop done
  ~temp-2
after
back
== keyedForeach
  at a
  at b
after loop
  ~keyed-a
after clear
  ~keyed-b
back
== intoArrayIndex
before
  ~held
after
  ~index-0
back
== locals
returning
  ~local-x
  ~local-y
5
back
== returnsLast
  ~returned-away
returned-away
== resurrection
  ~lazarus
after release
kept: 1
released again
== nested order
before
  ~outer
  ~inner
after
== generator
  got 1
before
  finally
after
== catch with a destructor pending
  ~in-thrower
caught yes
  ~in-thrower
caught as Exception
== end
  ~service
