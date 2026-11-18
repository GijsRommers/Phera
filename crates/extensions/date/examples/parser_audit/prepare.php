<?php
// Pin every clock-dependent procedural call in the copied FILE section.
// The upstream files are never changed. Both runtimes execute this identical copy.
$source = file_get_contents($argv[1]);
$functions = ['strtotime', 'date', 'gmdate', 'idate', 'getdate', 'localtime',
    'mktime', 'gmmktime', 'time'];
$output = '';
foreach (token_get_all($source) as $token) {
    if (is_array($token)) {
        if ($token[0] === T_STRING && in_array(strtolower($token[1]), $functions, true)) {
            $output .= 'fixed_' . strtolower($token[1]);
        } else {
            $output .= $token[1];
        }
    } else {
        $output .= $token;
    }
}
$prefix = <<<'PHP'
<?php
function fixed_time() { return 1706704496; }
function fixed_strtotime($datetime, $baseTimestamp = 1706704496) {
    return strtotime($datetime, $baseTimestamp ?? 1706704496);
}
function fixed_date($format, $timestamp = 1706704496) {
    return date($format, $timestamp ?? 1706704496);
}
function fixed_gmdate($format, $timestamp = 1706704496) {
    return gmdate($format, $timestamp ?? 1706704496);
}
function fixed_idate($format, $timestamp = 1706704496) {
    return idate($format, $timestamp ?? 1706704496);
}
function fixed_getdate($timestamp = 1706704496) { return getdate($timestamp ?? 1706704496); }
function fixed_localtime($timestamp = 1706704496, $associative = false) {
    return localtime($timestamp ?? 1706704496, $associative);
}
function fixed_mktime($h, $i = null, $s = null, $m = null, $d = null, $y = null) {
    $base = getdate(1706704496);
    return mktime($h, $i ?? $base['minutes'], $s ?? $base['seconds'],
        $m ?? $base['mon'], $d ?? $base['mday'], $y ?? $base['year']);
}
function fixed_gmmktime($h, $i = null, $s = null, $m = null, $d = null, $y = null) {
    return gmmktime($h, $i ?? 34, $s ?? 56, $m ?? 1, $d ?? 31, $y ?? 2024);
}
PHP;
echo $prefix, "\ndate_default_timezone_set(", var_export($argv[2], true), ");\n?>", $output;
