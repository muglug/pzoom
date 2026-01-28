<?php
$_arr = [];

foreach ([0, 1] as $i) {
    $_arr[$i] = 1;
}

/** @var int<0,1> $j */
$j = 0;

echo $_arr[$j];
