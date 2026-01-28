<?php
if (rand(0,1)) {
  $a = ["a" => 1];
} else {
  $a = [2, 3];
}

if (isset($a[0])) {
    echo $a[0];
}
