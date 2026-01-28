<?php
if (rand(0,1)) {
  $a = ["a" => 1];
} else {
  $a = [2, 3];
}

if (array_key_exists("a", $a)) {
    echo $a[0];
}
