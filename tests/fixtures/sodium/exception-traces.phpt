--TEST--
Sodium reports module functions and clears exception trace arguments
--EXTENSIONS--
sodium
--INI--
zend.exception_ignore_args=0
--FILE--
<?php
var_dump(in_array('sodium_crypto_secretbox', get_extension_funcs('sodium'), true));

function wrong_message($message, $key) {
    sodium_crypto_shorthash($message, $key);
}

function wrong_key($message, $key) {
    sodium_crypto_shorthash($message, $key);
}

function show_trace($error) {
    echo get_class($error), "\n";
    foreach ($error->getTrace() as $frame) {
        echo $frame['function'], " ", count($frame['args']), "\n";
    }
}

try {
    wrong_message([], str_repeat('k', 16));
} catch (TypeError $error) {
    show_trace($error);
}

try {
    wrong_key('secret payload', 'secret key');
} catch (SodiumException $error) {
    show_trace($error);
}
?>
--EXPECT--
bool(true)
TypeError
sodium_crypto_shorthash 0
wrong_message 0
SodiumException
sodium_crypto_shorthash 0
wrong_key 0
