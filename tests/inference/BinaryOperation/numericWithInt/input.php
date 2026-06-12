<?php
/** @return numeric */
function getNumeric(){
    return 1;
}
$a = getNumeric();
$a++;
$b = getNumeric() * 2;
$c = 1 - getNumeric();
$d = 2;
$d -= getNumeric();
