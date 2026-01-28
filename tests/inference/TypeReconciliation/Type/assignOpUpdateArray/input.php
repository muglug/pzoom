<?php
$optgroup = ["a" => ""];

if (rand(0, 1)) {
    $optgroup["a"] .= "v";
}

if ($optgroup["a"] !== "") {}