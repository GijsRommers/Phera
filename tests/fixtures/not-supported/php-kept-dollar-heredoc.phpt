--TEST--
Deprecated heredoc interpolation retains PHP diagnostic line numbers
--FILE--
<?php
$name = 'value';
$key = 'name';
echo <<<TEXT
first line
${name}
${$key}
TEXT;
--EXPECTF--
Deprecated: Using ${var} in strings is deprecated, use {$var} instead in /private%s on line 5

Deprecated: Using ${expr} (variable variables) in strings is deprecated, use {${expr}} instead in /private%s on line 6
first line
value
value
