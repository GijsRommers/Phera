--TEST--
A scanner deprecation handler can abort later warnings and keeps the native trace
--FILE--
<?php
set_error_handler(function ($kind, $message, $file, $line) {
    echo $kind, " [", $file, "] ", $line, "\n";
    throw new RuntimeException($message);
});
$source = <<<'PHP'
<?php
(double) "\400";
PHP;
try {
    PhpToken::tokenize($source, TOKEN_PARSE);
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n", $error->getTraceAsString(), "\n";
}
--EXPECTF--
8192 [] 2
Non-canonical cast (double) is deprecated, use the (float) cast instead
#0 [internal function]: {closure:%s:2}(8192, 'Non-canonical c...', '', 2)
#1 %s(11): PhpToken::tokenize('<?php\n(double) ...', 1)
#2 {main}
