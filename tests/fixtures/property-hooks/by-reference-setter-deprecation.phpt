--TEST--
A reference-returning void setter emits PHP diagnostics
--FILE--
<?php
class C {
    public $n {
        &set {
            $this->n = $value;
        }
    }
}
$c = new C;
$c->n = 42;
var_dump($c->n);
--EXPECTF--
Deprecated: C::$n::set(): Returning by reference from a void function is deprecated in %s on line 4

Notice: Only variable references should be returned by reference in %s on line 6
int(42)
