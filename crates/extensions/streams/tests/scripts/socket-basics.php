<?php
var_dump(stream_socket_client('missing://example:1', $errno, $error, 0.01), $errno, $error);
// PHP's list depends on its OpenSSL build; these are in every build and in Phera.
var_dump(array_values(array_intersect(
    ['tcp', 'udp', 'unix', 'udg', 'ssl', 'tls', 'tlsv1.2', 'tlsv1.3'],
    stream_get_transports(),
)));
[$left, $right] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, STREAM_IPPROTO_IP);
var_dump(stream_get_meta_data($left), stream_is_local($left));
var_dump(stream_socket_get_name($left, false), stream_socket_get_name($left, true));
$read = ['pair' => $left];
$write = $except = null;
var_dump(stream_select($read, $write, $except, 0), array_keys($read));
var_dump(stream_set_timeout($left, 0, 1000), fread($left, 12), feof($left));
var_dump(socket_get_status($left)['timed_out']);
var_dump(socket_set_timeout($left, 1));
var_dump(socket_set_blocking($left, false), fread($left, 12));
var_dump(fwrite($right, "one\r\ntwo\nthree"));
$read = ['pair' => $left];
var_dump(stream_select($read, $write, $except, 1), array_keys($read));
var_dump(stream_get_line($left, 0, "\r\n"), fgets($left), fread($left, 5));
var_dump(stream_socket_shutdown($right, STREAM_SHUT_WR), fread($left, 1), feof($left));
fclose($left);
fclose($right);

$server = stream_socket_server('tcp://127.0.0.1:0', $errno, $error);
var_dump(is_resource($server), $errno, $error);
$local = stream_socket_get_name($server, false);
$client = stream_socket_client($local, $errno, $error, 2.0);
var_dump(is_resource($client), $errno, $error);
$connection = stream_socket_accept($server, 2, $peer);
var_dump($peer === stream_socket_get_name($client, false));
var_dump($local === stream_socket_get_name($connection, false));
var_dump(fwrite($client, "hello\n"), fgets($connection));
var_dump(fwrite($connection, "world\n"), fgets($client));
fclose($connection);
fclose($client);
fclose($server);

$server = stream_socket_server('udp://127.0.0.1:0', $errno, $error, STREAM_SERVER_BIND);
$local = stream_socket_get_name($server, false);
$client = stream_socket_client('udp://' . $local, $errno, $error, 2.0);
var_dump(stream_socket_sendto($client, 'datagram'));
var_dump(stream_socket_recvfrom($server, 64, STREAM_PEEK));
var_dump(stream_socket_recvfrom($server, 64, 0, $peer));
var_dump($peer === stream_socket_get_name($client, false));
var_dump(stream_socket_sendto($server, 'reply', 0, $peer));
var_dump(stream_socket_recvfrom($client, 64));
fclose($client);
fclose($server);

foreach (['tcp://portless', ''] as $target) {
    var_dump(stream_socket_client($target, $errno, $error, 0.01), $errno, $error);
}
foreach ([
    fn() => stream_socket_shutdown(fopen('php://memory', 'w+'), 4),
    fn() => stream_socket_recvfrom(fopen('php://memory', 'w+'), 0),
    fn() => stream_get_line(fopen('php://memory', 'w+'), -1),
    fn() => stream_socket_client('tcp://127.0.0.1:1', timeout: INF),
] as $case) {
    try {
        $case();
    } catch (Throwable $error) {
        echo $error::class, ' ', $error->getMessage(), "\n";
    }
}
