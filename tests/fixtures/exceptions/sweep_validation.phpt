--TEST--
Exceptions: sweep_validation
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
function validate_name(string $name): string
{
    if (strlen($name) < 3) {
        throw new ValidationFailure('name too short');
    }
    return $name;
}
$errors = [];
$accepted = [];
foreach (['Alice', 'Bo', 'Carol', 'X'] as $name) {
    try {
        $accepted[] = validate_name($name);
    } catch (ValidationFailure $failure) {
        $errors[] = $failure->getMessage();
    }
}
echo count($accepted), '|', count($errors), "\n";
foreach ($errors as $message) {
    echo $message, "\n";
}

--EXPECTF--
2|2
name too short
name too short
