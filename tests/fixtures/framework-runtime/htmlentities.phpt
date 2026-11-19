--TEST--
HTML named entities preserve pairs, document type and encoding
--FILE--
<?php
var_dump(htmlentities('fjord & é', ENT_HTML5));
var_dump(htmlentities('Æ © <"é">', ENT_QUOTES | ENT_HTML401));
var_dump(htmlentities('&copy; &excl; é', ENT_HTML5, 'UTF-8', false));
var_dump(htmlentities(" euros \x80", ENT_QUOTES, 'CP1252'));
var_dump(htmlentities('é', ENT_XML1));
--EXPECT--
string(25) "&fjlig;ord &amp; &eacute;"
string(43) "&AElig; &copy; &lt;&quot;&eacute;&quot;&gt;"
string(22) "&copy; &excl; &eacute;"
string(13) " euros &euro;"
string(2) "é"
