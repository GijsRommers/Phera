--TEST--
Evaluating a nowdoc attributes the call to the first body line
--FILE--
<?php
function inspect() { debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS); }
eval(<<<'CODE'
inspect();
CODE);
--EXPECTF--
#0 %s/nowdoc-eval.php(4) : eval()'d code(1): inspect()
#1 %s/nowdoc-eval.php(4): eval()
