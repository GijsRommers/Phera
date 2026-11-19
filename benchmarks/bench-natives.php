<?php
// Calls through the same string, array, date and reflection APIs as framework code.
function nativeBenchmark(int $iterations): int {
    $checksum = 0;
    $fields = new ArrayObject(['method' => 'GET', 'path' => '/api']);
    $date = new DateTimeImmutable('2026-01-01T00:00:00Z');
    $reflection = new ReflectionClass(ArrayObject::class);
    $line = 'get,api,ping';

    for ($i = 0; $i < $iterations; ++$i) {
        $parts = explode(',', $line);
        $route = str_replace('ping', 'status', implode('/', $parts));
        $request = sprintf('%s %s', $fields['method'], $route);
        $checksum += abs(-$i) + count($parts) + strlen($route) + ord('a');
        $checksum += is_string($request) + in_array('api', $parts, true);
        $checksum += array_key_exists('path', $fields->getArrayCopy());
        $checksum += preg_match('/^GET /', $request);
        $checksum += $fields->count() + (int) $date->format('Y');
        $checksum += strlen($reflection->getName());
        $checksum += count(array_map('strtoupper', $parts));
    }

    return $checksum;
}

echo nativeBenchmark(50000), "\n";
