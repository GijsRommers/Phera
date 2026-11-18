<?php
// Rule: variable.undefined; deliberate single error: variable_try_possible.
function f($flag) { try { if ($flag) { throw new Exception; } $value = 1; } catch (Throwable $error) {} echo $value; }
