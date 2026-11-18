<?php
// Rule: variable.undefined; member bad this static arrow.
class C { function f() { $c = static fn() => $this; } } // error0
