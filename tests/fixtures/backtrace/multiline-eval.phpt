--TEST--
Multiline eval uses the source line at the end of its operand
--FILE--
<?php
function inspect() { debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS); }
eval("inspect();
"
)
;
--EXPECTF--
#0 %s/multiline-eval.php(3) : eval()'d code(1): inspect()
#1 %s/multiline-eval.php(3): eval()
