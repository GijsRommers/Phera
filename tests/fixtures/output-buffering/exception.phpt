--TEST--
A chunk handler exception passes through its original input
--FILE--
<?php
ob_start();
ob_start(fn($text, $mode) => throw new RuntimeException("handler failed"), 3);
try {
    echo 'abc';
} catch (RuntimeException $error) {
    echo ':', $error->getMessage();
}
ob_end_clean();
echo ob_get_clean(), "\n";
--EXPECT--
abc:handler failed
