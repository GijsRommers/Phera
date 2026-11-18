--TEST--
Compiling a file reports octal overflow once at each literal
--FILE--
<?php
echo bin2hex("\400"), "\n";
echo bin2hex(<<<DOC
    \401
    DOC
), "\n";
echo <<<'DOC'
\402
DOC;
echo "\n";
--EXPECTF--

Warning: Octal escape sequence overflow \400 is greater than \377 in %s on line 2

Warning: Octal escape sequence overflow \401 is greater than \377 in %s on line 4
00
01
\402
