<?php
// Rule: method.notFound; member bad missing typed method.
class C {} function f(C $c) { $c->missing(); } // error0
