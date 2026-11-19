--TEST--
OpenSSL native stack traces wrap sensitive parameters
--EXTENSIONS--
openssl
--FILE--
<?php
ini_set('zend.exception_ignore_args', '0');
try {
    openssl_encrypt('payload', 'aes-128-cbc', 'passphrase', tag_length: PHP_INT_MAX);
} catch (ValueError $error) {
    $arguments = $error->getTrace()[0]['args'];
    var_dump($arguments[0] instanceof SensitiveParameterValue);
    var_dump($arguments[0]->getValue() === 'payload');
    var_dump($arguments[2] instanceof SensitiveParameterValue);
    var_dump($arguments[2]->getValue() === 'passphrase');
}
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
