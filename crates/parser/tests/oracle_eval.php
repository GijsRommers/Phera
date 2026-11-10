<?php
// Evaluates expression pairs read from stdin, one pair per line:
// "<hex original> <hex parenthesised>" (the second may be empty).
// For each line it prints two lines: the result of the original, then the
// result of the parenthesised form ("SKIP" when absent). A result is
// "OK <hex var_export>" or "ERR <exception class>".
error_reporting(0);

function eval_line(string $code): string {
    try {
        $result = eval('return ' . $code . ';');
        return 'OK ' . bin2hex(var_export($result, true));
    } catch (Throwable $e) {
        return 'ERR ' . get_class($e);
    }
}

while (($line = fgets(STDIN)) !== false) {
    [$a, $b] = array_pad(explode(' ', trim($line), 2), 2, '');
    echo eval_line(hex2bin($a)), "\n";
    echo ($b === '' ? 'SKIP' : eval_line(hex2bin($b))), "\n";
}
