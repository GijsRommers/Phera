<?php
// Rule: variable.undefined; variable good capture.
$x = 1; $f = function () use ($x) { echo $x; };
