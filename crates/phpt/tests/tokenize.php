<?php
// Prints PHP's own tokens for each file path read from stdin, for comparison
// with Phera's lexer. Output per file: "@@ <path>", then one "<name> <hex text>"
// line per token.

// Tokenizer warnings (e.g. bad escapes) would otherwise end up in the output.
error_reporting(0);

while (($path = fgets(STDIN)) !== false) {
    $path = rtrim($path, "\n");
    echo "@@ $path\n";

    foreach (token_get_all(file_get_contents($path)) as $token) {
        [$name, $text] = is_array($token) ? [token_name($token[0]), $token[1]] : [$token, $token];
        echo $name, ' ', bin2hex($text), "\n";
    }
}
