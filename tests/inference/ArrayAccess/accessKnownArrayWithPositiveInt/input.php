<?php
/** @param list<int> $arr */
function foo(array $arr) : void {
    $o = [4, 15, 18, 21, 51];
    $i = 0;
    foreach ($arr as $a) {
        if ($o[$i] === $a) {}
        $i++;
    }
}
