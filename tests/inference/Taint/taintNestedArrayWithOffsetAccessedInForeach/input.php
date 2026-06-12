<?php
$a = [];
$a[0] = ["a" => $_GET["name"], "b" => "foo"];

foreach ($a as $m) {
    echo $m["a"];
}
