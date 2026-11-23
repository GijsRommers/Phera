--TEST--
Protected property access names the class of the object it is reached through
--FILE--
<?php
class Plain { protected $prot = 'q'; }
class Child extends Plain { }
class Other extends Plain { }

echo "read: ";
try { $c = new Child; echo $c->prot; } catch (Error $e) { echo $e->getMessage(); }
echo "\n";

echo "write: ";
try { $c = new Child; $c->prot = 1; echo "no error"; } catch (Error $e) { echo $e->getMessage(); }
echo "\n";

echo "isset: ";
$c = new Child;
var_dump(isset($c->prot));

echo "empty: ";
$c = new Child;
var_dump(empty($c->prot));

echo "unset: ";
try { $c = new Child; unset($c->prot); echo "no error"; } catch (Error $e) { echo $e->getMessage(); }
echo "\n";

echo "inc: ";
try { $c = new Child; $c->prot++; echo "no error"; } catch (Error $e) { echo $e->getMessage(); }
echo "\n";

echo "concat: ";
try { $c = new Child; $c->prot .= 'x'; echo "no error"; } catch (Error $e) { echo $e->getMessage(); }
echo "\n";

echo "append: ";
try { $c = new Child; $c->prot[] = 1; echo "no error"; } catch (Error $e) { echo $e->getMessage(); }
echo "\n";

echo "byref: ";
try { $c = new Child; $r = &$c->prot; echo "no error"; } catch (Error $e) { echo $e->getMessage(); }
echo "\n";

class Sibling extends Plain {
    public function read($o) { return $o->prot; }
    public function write($o) { $o->prot = 9; }
}
$s = new Sibling;
$c = new Child;
echo "sibling read: ", $s->read($c), "\n";
$s->write($c);
echo "sibling write: ok\n";

class Unrelated {
    public function touch($o) {
        try { echo $o->prot; } catch (Error $e) { echo $e->getMessage(); }
        echo "\n";
        try { $o->prot = 7; echo "write ok"; } catch (Error $e) { echo $e->getMessage(); }
        echo "\n";
    }
}
$u = new Unrelated;
$c2 = new Child;
$u->touch($c2);

echo "same class: ";
try { $p = new Plain; echo $p->prot; } catch (Error $e) { echo $e->getMessage(); }
echo "\n";
--EXPECT--
read: Cannot access protected property Child::$prot
write: Cannot access protected property Child::$prot
isset: bool(false)
empty: bool(true)
unset: Cannot access protected property Child::$prot
inc: Cannot access protected property Child::$prot
concat: Cannot access protected property Child::$prot
append: Cannot access protected property Child::$prot
byref: Cannot access protected property Child::$prot
sibling read: q
sibling write: ok
Cannot access protected property Child::$prot
Cannot access protected property Child::$prot
same class: Cannot access protected property Plain::$prot
