--TEST--
IPv4 integers and packed IPv4 and IPv6 addresses
--FILE--
<?php
foreach (['127.0.0.1', '255.255.255.255', '127.1', '::1', '::ffff:192.0.2.1', '2001:db8::1', 'bad'] as $input) {
    $packed = inet_pton($input);
    var_dump(ip2long($input), $packed === false ? false : bin2hex($packed));
    if ($packed !== false) var_dump(inet_ntop($packed));
}
var_dump(long2ip(-1), long2ip(4294967296), inet_ntop('short'));
--EXPECT--
int(2130706433)
string(8) "7f000001"
string(9) "127.0.0.1"
int(4294967295)
string(8) "ffffffff"
string(15) "255.255.255.255"
bool(false)
bool(false)
bool(false)
string(32) "00000000000000000000000000000001"
string(3) "::1"
bool(false)
string(32) "00000000000000000000ffffc0000201"
string(16) "::ffff:192.0.2.1"
bool(false)
string(32) "20010db8000000000000000000000001"
string(11) "2001:db8::1"
bool(false)
bool(false)
string(15) "255.255.255.255"
string(7) "0.0.0.0"
bool(false)
