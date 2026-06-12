<?php
$d = [];
if (!rand(0,1)) {
    $d[0] = "a";
}

$x = $d[0];

//  should not report TypeDoesNotContainNull
if ($x === null) {}
