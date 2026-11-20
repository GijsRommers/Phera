--TEST--
range converts Stringable endpoints in order and rejects non-Stringable objects
--EXTENSIONS--
gmp
--FILE--
<?php
final class EndPoint {
    function __construct(private string $text) {}
    function __toString(): string { echo 'cast ', $this->text, "\n"; return $this->text; }
}
echo json_encode(range(new EndPoint('a'), new EndPoint('c'))), "\n";
echo json_encode(range(new EndPoint('3'), 5)), "\n";
try { range(new EndPoint('a'), new DateTime('2020-01-01')); }
catch (TypeError $error) { echo $error->getMessage(), "\n"; }
echo json_encode(range(gmp_init(5), 3)), "\n";
--EXPECT--
cast a
cast c
["a","b","c"]
cast 3
[3,4,5]
cast a
range(): Argument #2 ($end) must be of type string|int|float, DateTime given
[5,4,3]
