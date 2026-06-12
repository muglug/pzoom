<?php
/**
 * @param array<array-key, int> $i
 * @param array<int, string> $arr
 */
function foo(array $i, array $arr) : void {
    foreach ($i as $j => $k) {
        echo $arr[$j];
    }
}
