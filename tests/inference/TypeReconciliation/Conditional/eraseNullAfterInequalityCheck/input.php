<?php
$a = mt_rand(0, 1) ? mt_rand(-10, 10) : null;

if ($a > 0) {
  echo $a + 3;
}

if (0 < $a) {
  echo $a + 3;
}