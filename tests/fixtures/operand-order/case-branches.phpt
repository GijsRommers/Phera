--TEST--
Case comparison branches keep evaluation order, fallthrough and warning lines
--FILE--
<?php
function caseValue($value) {
    echo 'compare ', get_debug_type($value), ' ', $value, "\n";
    return $value;
}

function runCases(): void {
    try {
        switch ('7') {
            case caseValue(6): echo "miss\n"; break;
            case caseValue(7): echo "loose\n";
            case caseValue(8): echo "fall through\n"; break;
        }
        echo match ('7') {
            caseValue(7) => "not strict\n",
            caseValue('7') => "strict\n",
        };
        echo match ($missing) { true => "true\n", default => "default\n" };
    } catch (ErrorException $error) {
        echo $error->getMessage(), ' on line ', $error->getLine(), "\n";
    } finally {
        echo "finally\n";
    }
}
set_error_handler(static function ($severity, $message, $file, $line) {
    throw new ErrorException($message, 0, $severity, $file, $line);
});
runCases();
--EXPECT--
compare int 6
compare int 7
loose
fall through
compare int 7
compare string 7
strict
Undefined variable $missing on line 18
finally
