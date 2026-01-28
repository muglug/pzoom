<?php
/** @var negative-int */
$int = -1;
/** @var array<int<min, -1>, int<min, -1>> */
$_arr = [];

$_arr[-1] = $int;
$_arr[$int] = -2;
