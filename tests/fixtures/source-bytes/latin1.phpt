--TEST--
ISO-8859-1 source echoes its original accented bytes
--FILE--
<?php
echo "Café déjà vu, année et façade.
";
--EXPECT--
Café déjà vu, année et façade.
