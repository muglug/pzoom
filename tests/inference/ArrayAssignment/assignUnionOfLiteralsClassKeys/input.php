<?php
class a {}
class b {}

$result = [];

foreach ([a::class, b::class] as $k) {
    $result[$k] = true;
}

foreach ($result as $k => $v) {
    $vv = new $k;
}
