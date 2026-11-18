
$oracleDiagnostics = [];
set_error_handler(function ($severity, $message) {
    global $oracleDiagnostics;
    $oracleDiagnostics[] = [$severity, $message];
    return true;
});

function oracleValue($value) {
    if (is_array($value)) {
        $encoded = [];
        foreach ($value as $key => $item) {
            $encoded[$key] = oracleValue($item);
        }
        return $encoded;
    }
    if (is_object($value)) {
        if ($value instanceof ReflectionType) {
            return [get_class($value), $value->__toString()];
        }
        return [get_class($value), $value->name ?? null, $value->class ?? null];
    }
    return $value;
}

function oracleCall($key, $factory, $method, $arguments) {
    global $oracleDiagnostics;
    $oracleDiagnostics = [];
    $output = '';
    try {
        $reflector = $factory();
        $value = $reflector->$method(...$arguments);
        $output = ['value', $method === 'getId' ? strlen($value) : oracleValue($value)];
    } catch (Throwable $error) {
        $output = ['error', get_class($error), $error->getMessage()];
    }
    echo 'CASE ', $key, '=', json_encode([$output, $oracleDiagnostics], JSON_INVALID_UTF8_SUBSTITUTE), "\n";
}
