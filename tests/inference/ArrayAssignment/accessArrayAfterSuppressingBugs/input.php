<?php
$a = [];

foreach (["one", "two", "three"] as $key) {
    $a[$key] ??= 0;
    $a[$key] += rand(0, 10);
}

$a["four"] = true;

if ($a["one"]) {}
