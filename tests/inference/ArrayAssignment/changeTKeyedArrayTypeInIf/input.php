<?php
$a = [];

if (rand(0, 5) > 3) {
  $a["b"] = new stdClass;
} else {
  $a["b"] = ["e" => "f"];
}

if ($a["b"] instanceof stdClass) {
  $a["b"] = [];
}

$a["b"]["e"] = "d";
