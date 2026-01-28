<?php
/** @return int<0, 10> */
function getInt(): int{ return rand(0, 10); }

$a = getInt();
$b = -$a;
$c = null;
if($b === $a){
    //$b and $a should intersect at 0, so $c should be 0
    $c = $b;
}
                    
