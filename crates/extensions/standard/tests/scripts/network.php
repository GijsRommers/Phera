<?php
function ipv4Loopback($address): bool {
    return is_string($address) && str_starts_with($address, '127.')
        && inet_pton($address) !== false;
}

var_dump(ipv4Loopback(gethostbyname('localhost')));
var_dump(count(array_filter(gethostbynamel('localhost') ?: [], 'ipv4Loopback')) > 0);
foreach (['127.0.0.1', '192.0.2.91', '', '::1'] as $name) {
    var_dump(gethostbyname($name), gethostbynamel($name));
}

var_dump(getservbyname('http', 'tcp'), getservbyname('phera-absent', 'tcp'));
$service = getservbyport(80, 'tcp');
var_dump(is_string($service) ? getservbyname($service, 'tcp') : false);
var_dump(getservbyport(65616, 'tcp') === $service);
var_dump(getprotobyname('tcp'), getprotobyname('phera-absent'));
$protocol = getprotobynumber(6);
var_dump(is_string($protocol) ? getprotobyname($protocol) : false);
var_dump(getprotobynumber(999));

$loopback = false;
foreach (net_get_interfaces() as $interface) {
    foreach ($interface['unicast'] as $address) {
        if (ipv4Loopback($address['address'] ?? false)) {
            $loopback = $interface['up'] && isset($address['netmask'])
                && is_int($address['flags']) && is_int($address['family']);
        }
    }
}
var_dump($loopback);

foreach ([
    fn() => gethostbyname("a\0b"),
    fn() => gethostbynamel("a\0b"),
    fn() => getservbyname("http\0", 'tcp'),
    fn() => getservbyport(80, "tcp\0"),
    fn() => getprotobyname("tcp\0"),
] as $call) {
    try {
        $call();
    } catch (Throwable $error) {
        echo $error::class, ' ', $error->getMessage(), "\n";
    }
}

$reverse = gethostbyaddr('127.0.0.1');
var_dump(is_string($reverse) && $reverse !== '');
$ns = 'previous';
$extra = 'previous';
var_dump(dns_get_record('localhost', 0, $ns, $extra), $ns, $extra);
foreach ([
    fn() => gethostbyaddr("127.0.0.1\0"),
    fn() => dns_check_record(''),
    fn() => checkdnsrr('localhost', 'PHERA_INVALID_TYPE'),
    fn() => dns_get_record('localhost', -1, $ns, $extra),
    fn() => dns_get_record('localhost', 0, $ns, $extra, true),
    fn() => getmxrr("localhost\0", $ns, $extra),
    fn() => dns_get_mx("localhost\0", $ns, $extra),
] as $call) {
    try {
        $call();
    } catch (Throwable $error) {
        echo $error::class, ' ', $error->getMessage(), "\n";
    }
}
