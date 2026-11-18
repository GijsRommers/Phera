<?php
// Rule: control flow / variable.undefined / return; loop_known_once.
for ($i = 0; $i < 1; $i++) { $x = 1; }
echo $x;
