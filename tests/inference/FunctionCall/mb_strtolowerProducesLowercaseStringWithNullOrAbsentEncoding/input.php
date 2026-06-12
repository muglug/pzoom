<?php
/** @var non-empty-string $i */
$i = "École";
/** @var string $j */
$j = "";
$a = mb_strtolower($i);
$b = mb_strtolower($i, null);
$c = mb_strtolower($j);
$d = mb_strtolower($j, null);
$e = mb_strtolower("AAA");
$f = mb_strtolower("AAA", null);
$g = mb_strtolower("");
$h = mb_strtolower("", null);
