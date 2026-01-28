<?php
$x = ["key" => "value"];
if (rand(0, 1)) {
    $x = [];
}
if ($x) {
    var_export($x);
}