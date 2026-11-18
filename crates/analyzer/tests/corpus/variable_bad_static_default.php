<?php
// Rule: variable.undefined; variable bad static default.
function f() { static $x = $missing; } // error0
