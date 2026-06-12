<?php
$x1 = ["a" => "value"];
unset($x1["a"]);

$x2 = ["a" => "value", "b" => "value"];
unset($x2["a"]);

$x3 = ["a" => "value", "b" => "value"];
$k = "a";
unset($x3[$k]);
