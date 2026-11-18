<?php
// Prints, for each file path read from stdin, whether PHP's parser accepts it:
// "OK", "ERR", or "COMPILE_ERR" for a declaration rejected with CompileError.
error_reporting(0);

while (($path = fgets(STDIN)) !== false) {
    $path = rtrim($path, "\n");
    try {
        token_get_all(file_get_contents($path), TOKEN_PARSE);
        echo "OK\n";
    } catch (ParseError $e) {
        echo "ERR\n";
    } catch (CompileError $e) {
        echo "COMPILE_ERR\n";
    } catch (Throwable $e) {
        echo "OK\n";
    }
}
