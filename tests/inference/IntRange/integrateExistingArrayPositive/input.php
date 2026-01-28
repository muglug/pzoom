<?php
/** @return int<5, max> */
function getInt()
{
    return 7;
}

$_arr = ["a", "b", "c"];
$a = getInt();
$_arr[$a] = 12;
