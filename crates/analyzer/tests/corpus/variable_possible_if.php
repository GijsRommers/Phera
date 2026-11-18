<?php
// Rule: variable.undefined; variable possible if.
if (rand()) { $x = 1; }
echo $x; // error1
