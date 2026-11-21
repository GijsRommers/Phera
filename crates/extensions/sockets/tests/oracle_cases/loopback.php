<?php

function item($label, $value) {
    echo $label, '=', json_encode($value, JSON_THROW_ON_ERROR), "\n";
}
function address($socket, $peer = false) {
    $f = $peer ? 'socket_getpeername' : 'socket_getsockname';
    $f($socket, $host, $port);
    return [$host, $port];
}

foreach (['Socket', 'AddressInfo'] as $class) {
    try {
        new $class();
    } catch (Error $error) {
        item($class, $error->getMessage());
    }
}

item('pair', socket_create_pair(AF_UNIX, SOCK_STREAM, 0, $pair));
[$a, $b] = $pair;
item('nonblocking', socket_set_nonblock($a));
item('empty', socket_recv($a, $data, 8, 0));
item('empty data', $data);
item('would block', in_array(socket_last_error($a), [SOCKET_EAGAIN, SOCKET_EWOULDBLOCK], true));
item('same error', socket_last_error() === socket_last_error($a));
socket_clear_error($a);
item('clear', socket_last_error($a));
item('send', socket_send($b, "one\rtwo", 7, 0));
$read = [3 => $a, 'again' => $a];
$write = $except = null;
item('select', socket_select($read, $write, $except, 0));
item('ready keys', array_keys($read));
item('read line', socket_read($a, 10, PHP_NORMAL_READ));
item('peek count', socket_recv($a, $data, 10, MSG_PEEK));
item('peek data', $data);
item('read bytes', socket_read($a, 3));
item('export id', socket_export_stream($a) === socket_export_stream($a));
$stream = socket_export_stream($a);
item('block', socket_set_block($a));
item('stream blocked', stream_get_meta_data($stream)['blocked']);
item('stream write', fwrite($stream, "reply"));
item('socket read', socket_read($b, 5));
socket_close($a);
item('stream closed', !is_resource($stream));
try {
    socket_read($a, 1);
} catch (Error $error) {
    item('socket closed', $error->getMessage());
}
socket_close($b);

$server = socket_create(AF_INET, SOCK_STREAM, SOL_TCP);
item('set option', socket_set_option($server, SOL_SOCKET, SO_REUSEADDR, 1));
item('get option', socket_getopt($server, SOL_SOCKET, SO_REUSEADDR) > 0);
item('set timeout', socket_setopt($server, SOL_SOCKET, SO_RCVTIMEO, ['sec' => 1, 'usec' => 0]));
item('get timeout', socket_get_option($server, SOL_SOCKET, SO_RCVTIMEO));
item('bind', socket_bind($server, '127.0.0.1', 0));
item('listen', socket_listen($server));
[$host, $port] = address($server);
item('listener address', [$host, $port > 0]);
$hints = ['ai_family' => AF_INET, 'ai_socktype' => SOCK_STREAM, 'ai_protocol' => SOL_TCP];
$records = socket_addrinfo_lookup($host, (string) $port, $hints);
$info = socket_addrinfo_explain($records[0]);
item('address info', [$info['ai_family'] === AF_INET, $info['ai_socktype'] === SOCK_STREAM,
    $info['ai_protocol'] === SOL_TCP, $info['ai_addr']['sin_port'] === $port,
    $info['ai_addr']['sin_addr']]);
$client = socket_addrinfo_connect($records[0]);
$serverSide = socket_accept($server);
item('accept', $serverSide instanceof Socket);
item('peer', address($serverSide, true) === address($client));
item('write', socket_write($client, 'test', 99));
item('read', socket_read($serverSide, 4));
item('shutdown', socket_shutdown($client, SHUT_WR));
item('eof', socket_recv($serverSide, $data, 8, 0));
item('eof data', $data);
socket_close($client);
socket_close($serverSide);
socket_close($server);

try {
    socket_addrinfo_lookup('127.0.0.1', '80', ['ai_socktype' => new stdClass()]);
} catch (TypeError $error) {
    item('address info hint type', $error->getMessage());
}
try {
    socket_addrinfo_lookup('127.0.0.1', '80', ['ai_family' => PHP_INT_MAX]);
} catch (ValueError $error) {
    item('address info family', get_class($error));
}

$opts = stream_context_create(['socket' => ['tcp_nodelay' => true]]);
$listener = stream_socket_server('tcp://127.0.0.1:0', $error, $message, STREAM_SERVER_BIND | STREAM_SERVER_LISTEN, $opts);
$client = stream_socket_client('tcp://' . stream_socket_get_name($listener, false), $error, $message, 2, STREAM_CLIENT_CONNECT, $opts);
$accepted = stream_socket_accept($listener, 2);
foreach (['listener' => $listener, 'client' => $client, 'accepted' => $accepted] as $side => $tcp) {
    $socket = socket_import_stream($tcp);
    item('TCP_NODELAY ' . $side, socket_get_option($socket, SOL_TCP, TCP_NODELAY) > 0);
    fclose($tcp);
}

$receiver = socket_create(AF_INET, SOCK_DGRAM, SOL_UDP);
$sender = socket_create(AF_INET, SOCK_DGRAM, SOL_UDP);
socket_bind($receiver, '127.0.0.1', 0);
socket_bind($sender, '127.0.0.1', 0);
[$host, $port] = address($receiver);
item('sendto', socket_sendto($sender, 'UDP', 3, 0, $host, $port));
item('recvfrom', socket_recvfrom($receiver, $data, 8, 0, $from, $fromPort));
item('packet', [$data, [$from, $fromPort] === address($sender)]);
socket_close($sender);
socket_close($receiver);

socket_create_pair(AF_UNIX, SOCK_DGRAM, 0, $pair);
[$a, $b] = $pair;
$space = socket_cmsg_space(SOL_SOCKET, SCM_RIGHTS, 1);
item('control space', $space > 0);
item('sendmsg', socket_sendmsg($a, [
    'iov' => ['XY', 'Z'],
    'control' => [['level' => SOL_SOCKET, 'type' => SCM_RIGHTS, 'data' => [$b]]],
]));
$message = ['buffer_size' => 8, 'controllen' => $space];
item('recvmsg', socket_recvmsg($b, $message));
item('iov', $message['iov']);
$control = $message['control'][0];
item('received descriptor', [$control['level'] === SOL_SOCKET, $control['type'] === SCM_RIGHTS,
    $control['data'][0] instanceof Socket]);
item('write through descriptor', socket_write($control['data'][0], 'row'));
item('read through pair', socket_read($a, 3));
