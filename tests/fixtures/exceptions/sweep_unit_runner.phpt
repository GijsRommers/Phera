--TEST--
Exceptions: sweep_unit_runner
--FILE--
<?php
class ApplicationFailure extends RuntimeException {}
class ValidationFailure extends ApplicationFailure {}
class TransportFailure extends ApplicationFailure {}
function error_response(Throwable $failure): array
{
    return [
        'status' => 500,
        'error' => get_class($failure),
        'message' => $failure->getMessage(),
    ];
}
function print_response(array $response): void
{
    echo $response['status'], '|';
    echo $response['error'], '|';
    echo $response['message'], "\n";
}
function test_success(): void
{
    if (2 + 2 != 4) throw new ApplicationFailure('arithmetic');
}
function test_failure(): void
{
    throw new ApplicationFailure('expected failure');
}
$passed = 0;
foreach (['test_success', 'test_failure', 'test_success'] as $test) {
    try {
        $test();
        echo $test, ':pass', "\n";
        $passed++;
    } catch (Throwable $failure) {
        echo $test, ':fail:', $failure->getMessage(), "\n";
    }
}
echo 'passed:', $passed;

--EXPECTF--
test_success:pass
test_failure:fail:expected failure
test_success:pass
passed:2