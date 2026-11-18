<?php
// Rule: variable.undefined; variable good byref capture.
$f = function () use (&$x) { echo $x; }; echo $x;
