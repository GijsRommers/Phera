<?php
// Rule: method.notFound; member bad missing property method.
class C {} class A { public C $p; function f() { $this->p->missing(); } } // error0
