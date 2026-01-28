<?php
$string = "c";
$int = 5;

$b = [];

$b[0][$string] = 5;
$b[0][0] = 3;

$c = [];

$c[0][0] = 3;
$c[0][$string] = 5;

$d = [];

$d[0][$int] = 3;
$d[0]["a"] = 5;

$e = [];

$e[0][$int] = 3;
$e[0][$string] = 5;
