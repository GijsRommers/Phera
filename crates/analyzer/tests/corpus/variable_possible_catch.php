<?php
// Rule: variable.undefined; variable possible catch.
try { strlen("a"); } catch (Exception $e) {}
echo $e; // error1
