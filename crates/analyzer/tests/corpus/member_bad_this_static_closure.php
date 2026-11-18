<?php
// Rule: variable.undefined; member bad this static closure.
class C { function f() { $c = static function () { echo $this; }; } } // error0
