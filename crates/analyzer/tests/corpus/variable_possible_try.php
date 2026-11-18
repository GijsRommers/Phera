<?php
// Rule: variable.undefined; variable possible try.
try { if (rand()) { throw new Exception; } } catch (Exception $e) { $x = 1; }
echo $x; // error1
