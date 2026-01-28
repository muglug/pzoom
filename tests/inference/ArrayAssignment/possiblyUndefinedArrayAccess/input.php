<?php
if (rand(0,1)) {
  $a = ["a" => 1];
} else {
  $a = [2, 3];
}

echo $a[0];
