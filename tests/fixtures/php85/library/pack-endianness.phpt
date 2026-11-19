--TEST--
Integer and floating endian modifiers precede the repeat count or field name
--FILE--
<?php
foreach ([
    fn() => bin2hex(pack('s>2S<l>Q<f>d<', -2, 258, 258, -2, 258, 1.5, -2.5)),
    fn() => unpack('s>signed/S<unsigned/f>float/d<double', hex2bin('fffe02013fc0000000000000000004c0')),
    fn() => unpack('s>2item', hex2bin('fffe0102')),
    fn() => pack('n>', 1),
    fn() => pack('C>', 1),
    fn() => unpack('n>name', "\0\1"),
    fn() => unpack('C>name', "\1"),
] as $call) {
    try { var_dump($call()); }
    catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
}
--EXPECT--
Type >: unknown format code
array(4) {
  [">signed"]=>
  int(-257)
  ["<unsigned"]=>
  int(258)
  [">float"]=>
  float(6.896490392174587E-41)
  ["<double"]=>
  float(-2.5)
}
array(1) {
  [">2item"]=>
  int(-257)
}
Type >: unknown format code
Type >: unknown format code
array(1) {
  [">name"]=>
  int(1)
}
array(1) {
  [">name"]=>
  int(1)
}
