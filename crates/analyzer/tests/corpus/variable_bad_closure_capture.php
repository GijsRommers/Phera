<?php
// Rule: variable.undefined; variable bad closure capture.
$f = function () use ($x) {}; // error0
