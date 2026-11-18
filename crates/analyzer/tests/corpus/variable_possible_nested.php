<?php
// Rule: variable.undefined; variable possible nested.
if (rand()) { if (rand()) { $x = 1; } }
echo $x; // error1
