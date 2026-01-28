<?php
/** @var non-positive-int */
$int = -1;
/** @var array<int<min, 0>, int<min, 0>> */
$_arr = [];

$_arr[0] = 0;
$_arr[-1] = $int;
$_arr[$int] = -2;
