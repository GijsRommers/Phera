<?php
// Rule: variable.undefined; variable possible while.
while (rand()) { $x = 1; }
echo $x; // error1
