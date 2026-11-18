<?php
// Rule: variable.undefined; variable possible else.
if (rand()) {} else { $x = 1; }
echo $x; // error1
