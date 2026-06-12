<?php
/** @var mixed $a */
$a;
/** @var never $b */
$b;
/** @var never $c */
$c;
/** @var never $d */
$d;
if (is_int($a)) {
    $b = $a;
}
if (is_integer($a)) {
    $c = $a;
}
if (is_long($a)) {
    $d = $a;
}
