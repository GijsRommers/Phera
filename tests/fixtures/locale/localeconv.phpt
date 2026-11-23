--TEST--
C locale uses an ASCII decimal point and no grouping
--FILE--
<?php
setlocale(LC_ALL, 'C');
$locale = localeconv();
var_dump($locale['decimal_point'], $locale['currency_symbol'], $locale['grouping']);
echo implode(',', array_keys($locale)), "\n";
--EXPECT--
string(1) "."
string(0) ""
array(0) {
}
decimal_point,thousands_sep,int_curr_symbol,currency_symbol,mon_decimal_point,mon_thousands_sep,positive_sign,negative_sign,int_frac_digits,frac_digits,p_cs_precedes,p_sep_by_space,n_cs_precedes,n_sep_by_space,p_sign_posn,n_sign_posn,grouping,mon_grouping
