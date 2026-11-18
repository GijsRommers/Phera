<?php
// Rule: variable.undefined; variable bad closure read.
$f = function () { echo $x; }; // error0
