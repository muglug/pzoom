<?php
/**
 * @param numeric-string $arg
 * @return void
 */
function takesNumeric($arg) {}

$b = rand(0, 10);
$a = $b < 5 ? "" : (string) $b;
if ($a !== "") {
    takesNumeric($a);
}

/** @var ""|numeric-string $c */
if (is_numeric($c)) {
    takesNumeric($c);
}
