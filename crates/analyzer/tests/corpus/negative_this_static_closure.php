<?php
// Rule: variable.undefined; deliberate single error: this_static_closure.
$f = static function () { return $this; };
