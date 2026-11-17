--TEST--
Exceptions: sweep_deep_parser
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
function parse_groups(string $input, int $position, int $depth): int
{
    if ($position >= strlen($input)) {
        if ($depth != 0) {
            throw new ValidationFailure('unclosed group');
        }
        return $position;
    }
    $character = $input[$position];
    if ($character == '(') {
        return parse_groups($input, $position + 1, $depth + 1);
    }
    if ($character == ')') {
        return parse_groups($input, $position + 1, $depth - 1);
    }
    return parse_groups($input, $position + 1, $depth);
}
try {
    echo parse_groups(str_repeat('(', 200), 0, 0);
} catch (ValidationFailure $failure) {
    print_response(error_response($failure));
}

--EXPECTF--
500|ValidationFailure|unclosed group
