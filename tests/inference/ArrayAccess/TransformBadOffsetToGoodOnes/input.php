<?php
$index = 1.1;

/** @psalm-suppress InvalidArrayOffset */
$_arr1 = [$index => 5];

$_arr2 = [];
/** @psalm-suppress InvalidArrayOffset */
$_arr2[$index] = 5;
