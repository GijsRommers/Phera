<?php
// Rule: variable.undefined; variable possible for.
for ($i = 0; $i < rand(0, 2); $i++) { $x = 1; }
echo $x; // error1
