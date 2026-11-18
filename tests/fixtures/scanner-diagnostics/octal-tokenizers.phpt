--TEST--
Raw and parsed tokenizers publish compile warnings at different locations
--FILE--
<?php
set_error_handler(function () {
    echo "Unexpected error handler call\n";
    return true;
});
$source = <<<'CODE'
<?php
<<<DOC
\400 {$value["\401"]} \402
DOC;
CODE;
echo "raw\n";
token_get_all($source);
PhpToken::tokenize($source);
echo "parsed\n";
token_get_all($source, TOKEN_PARSE);
PhpToken::tokenize($source, TOKEN_PARSE);
echo "suppressed\n";
@token_get_all($source);
error_reporting(0);
PhpToken::tokenize($source, TOKEN_PARSE);
echo "done\n";
--EXPECTF--
raw

Warning: Octal escape sequence overflow \401 is greater than \377 in %s on line 13

Warning: Octal escape sequence overflow \401 is greater than \377 in %s on line 14
parsed

Warning: Octal escape sequence overflow \400 is greater than \377 in  on line 3

Warning: Octal escape sequence overflow \401 is greater than \377 in  on line 3

Warning: Octal escape sequence overflow \402 is greater than \377 in  on line 3

Warning: Octal escape sequence overflow \400 is greater than \377 in  on line 3

Warning: Octal escape sequence overflow \401 is greater than \377 in  on line 3

Warning: Octal escape sequence overflow \402 is greater than \377 in  on line 3
suppressed
done
