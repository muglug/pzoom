<?php
$arr = [[1, 2, 3], null, [1, 2, 3], null];
$b = 2;
$c = 0;
if (isset($arr[$b][$c])) {
    $b = 1;
    echo $arr[$b][$c];
}
