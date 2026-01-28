<?php
$arrayA = [1, 2, 3];
$arrayB = [4, 5];
$result = [0, ...$arrayA, ...$arrayB, 6 ,7];

$arr1 = [3 => 1, 1 => 2, 3];
$arr2 = [...$arr1];
$arr3 = [1 => 0, ...$arr1];
