--TEST--
PHP source tools match highlight_string
--FILE--
<?php
$code = <<<'SOURCE'
<div>&</div>
<?php
/** documentation */
class Example {
	public static function show() { return 'text<&>'; }
}
echo true, null, __LINE__, "{$value->name}";
?>
SOURCE;
echo json_encode(highlight_string($code, return: true), JSON_UNESCAPED_SLASHES), "\n";
ini_set('highlight.html', 'black');
ini_set('highlight.default', 'blue');
ini_set('highlight.keyword', 'green');
ini_set('highlight.string', 'red');
ini_set('highlight.comment', 'orange');
echo highlight_string("a<&\n<?php echo 'x'; // hi\n?>\n", true), "\n";
ob_start();
$returned = highlight_string('<?= 1 ?>');
$output = ob_get_clean();
var_dump($returned, $output);
echo highlight_string('<?php echo "unterminated ?>', true), "\n";

--EXPECT--
"<pre><code style=\"color: #000000\">&lt;div&gt;&amp;&lt;/div&gt;\n<span style=\"color: #0000BB\">&lt;?php\n</span><span style=\"color: #FF8000\">/** documentation */\n</span><span style=\"color: #007700\">class </span><span style=\"color: #0000BB\">Example </span><span style=\"color: #007700\">{\n    public static function </span><span style=\"color: #0000BB\">show</span><span style=\"color: #007700\">() { return </span><span style=\"color: #DD0000\">'text&lt;&amp;&gt;'</span><span style=\"color: #007700\">; }\n}\necho </span><span style=\"color: #0000BB\">true</span><span style=\"color: #007700\">, </span><span style=\"color: #0000BB\">null</span><span style=\"color: #007700\">, </span><span style=\"color: #0000BB\">__LINE__</span><span style=\"color: #007700\">, </span><span style=\"color: #DD0000\">\"</span><span style=\"color: #007700\">{</span><span style=\"color: #0000BB\">$value</span><span style=\"color: #007700\">-&gt;</span><span style=\"color: #0000BB\">name</span><span style=\"color: #007700\">}</span><span style=\"color: #DD0000\">\"</span><span style=\"color: #007700\">;\n</span><span style=\"color: #0000BB\">?&gt;</span></code></pre>"
<pre><code style="color: black">a&lt;&amp;
<span style="color: blue">&lt;?php </span><span style="color: green">echo </span><span style="color: red">'x'</span><span style="color: green">; </span><span style="color: orange">// hi
</span><span style="color: blue">?&gt;
</span></code></pre>
bool(true)
string(92) "<pre><code style="color: black"><span style="color: blue">&lt;?= 1 ?&gt;</span></code></pre>"
<pre><code style="color: black"><span style="color: blue">&lt;?php </span><span style="color: green">echo </span><span style="color: red">"unterminated ?&gt;</span></code></pre>
