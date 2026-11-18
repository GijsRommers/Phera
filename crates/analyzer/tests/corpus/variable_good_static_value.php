<?php
// Rule: variable.undefined; variable good static value.
function f() { static $x = 1; echo $x; }
