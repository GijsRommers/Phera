<?php
// Rule: variable.undefined; variable possible finally.
try { if (rand(0, 1)) { throw new Exception; } $x = 1; } catch (Exception $e) {} finally {}
echo $x; // error1
