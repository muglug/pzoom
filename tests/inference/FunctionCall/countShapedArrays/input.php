<?php
/** @var array{a?: int} */
$a = [];
$aCount = count($a);

/** @var array{a: int} */
$b = [];
$bCount = count($b);

/** @var array{a: int, b?: int} */
$c = [];
$cCount = count($c);

/** @var array{a: int}&array */
$d = [];
$dCount = count($d);

/** @var list{0?: int} */
$e = [];
$eCount = count($e);

/** @var list{int} */
$f = [];
$fCount = count($f);

/** @var list{0: int, 1?: int} */
$g = [];
$gCount = count($g);

/** @var list{0: int, 1?: int}&array */
$h = [];
$hCount = count($h);
