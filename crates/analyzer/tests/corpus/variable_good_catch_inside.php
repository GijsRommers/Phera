<?php
// Rule: variable.undefined; variable good catch inside.
try { throw new Exception; } catch (Exception $e) { echo $e; }
