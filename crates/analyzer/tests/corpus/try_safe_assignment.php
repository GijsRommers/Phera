<?php
// Rule: scope / inheritance; try_safe_assignment.
try { $x = 1; } catch (Exception $e) {} echo $x;
