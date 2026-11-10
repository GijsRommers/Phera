<?php
// Prints, for each file path read from stdin, whether PHP's parser accepts it:
// "OK" or "ERR". Only ParseError counts as a rejection; a CompileError means
// the grammar accepted the code and the failure is later, at compile time.
error_reporting(0);

while (($path = fgets(STDIN)) !== false) {
    $path = rtrim($path, "\n");
    try {
        token_get_all(file_get_contents($path), TOKEN_PARSE);
        echo "OK\n";
    } catch (ParseError $e) {
        echo "ERR\n";
    } catch (Throwable $e) {
        echo "OK\n";
    }
}
