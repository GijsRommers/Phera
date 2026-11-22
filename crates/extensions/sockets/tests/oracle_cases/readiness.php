<?php
// Port numbers and OS error messages are deliberately left out of this comparison.
function peer(int $port): Socket {
    $socket = socket_create(AF_INET, SOCK_STREAM, SOL_TCP);
    socket_connect($socket, '127.0.0.1', $port);
    return $socket;
}

function result(string $label, int|false $count, array ...$sets): void {
    echo $label, ' ', json_encode([$count, ...array_map(array_keys(...), $sets)]), "\n";
}

$port = (int) $argv[1];
foreach (['close', 'reset'] as $action) {
    $socket = peer($port);
    $read = ['socket' => $socket];
    $write = $except = null;
    result("$action wait", socket_select($read, $write, $except, 1), $read);

    $read = $write = $except = ['socket' => $socket];
    result("$action all sets", socket_select($read, $write, $except, 0), $read, $write, $except);

    $read = $write = null;
    $except = ['socket' => $socket];
    result("$action except timeout", socket_select($read, $write, $except, 0, 40000), $except);
    socket_close($socket);
}

$socket = peer($port);
$read = $write = null;
$except = ['socket' => $socket];
result('urgent', socket_select($read, $write, $except, 1), $except);
socket_write($socket, '+');
socket_close($socket);
