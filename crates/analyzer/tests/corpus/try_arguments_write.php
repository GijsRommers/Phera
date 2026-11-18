<?php
// Rule: scope / inheritance; try_arguments_write.
try { strlen($x = "a"); } catch (Exception $e) { echo $x; }
