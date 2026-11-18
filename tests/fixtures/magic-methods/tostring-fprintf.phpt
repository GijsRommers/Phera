--TEST--
A stream format argument can run __toString before writing the formatted bytes
--FILE--
<?php
class StreamLabel {
    public function __toString(): string { return 'label'; }
}
$stream = fopen('php://temp', 'w+b');
var_dump(fprintf($stream, '%s', new StreamLabel));
rewind($stream);
echo stream_get_contents($stream), "\n";
--EXPECT--
int(5)
label
