<?php
// Rule: variable.undefined; deliberate single error: this_static_method.
class C { static function f() { return $this; } }
