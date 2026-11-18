<?php
// Rule: variable.undefined; variable good static.
function f() { static $x; echo $x; }
