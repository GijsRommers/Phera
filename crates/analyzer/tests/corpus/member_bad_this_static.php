<?php
// Rule: variable.undefined; member bad this static.
class C { static function f() { echo $this; } } // error0
