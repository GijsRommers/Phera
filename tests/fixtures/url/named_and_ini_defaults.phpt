--TEST--
Named web functions use PHP defaults and the request separator
--FILE--
<?php
var_dump(setcookie('name', httponly: true), setrawcookie('name', expires_or_options: ['secure' => true]));
$query = ['x' => 'a b', 'y' => true];
echo http_build_query($query, encoding_type: PHP_QUERY_RFC3986), "\n";
$old = ini_set('arg_separator.output', '&amp;');
echo $old, "\n", ini_get('arg_separator.output'), "\n", http_build_query($query), "\n";
echo http_build_query($query, arg_separator: ''), "\n";
foreach ([
    fn() => setcookie(httponly: true),
    fn() => parse_url(component: PHP_URL_PATH),
] as $call) {
    try {
        $call();
    } catch (ArgumentCountError $error) {
        echo $error->getMessage(), "\n";
    }
}
--EXPECT--
bool(true)
bool(true)
x=a%20b&y=1
&
&amp;
x=a+b&amp;y=1
x=a+by=1
setcookie(): Argument #1 ($name) not passed
parse_url(): Argument #1 ($url) not passed
