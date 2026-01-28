<?php
/** @return int<min, -1> */
function getInt()
{
    return -2;
}

$_arr = ["a", "b", "c"];
$a = getInt();
$_arr[$a] = 12;
