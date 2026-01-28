<?php
$a = false;

for ($i = 0; $i < 4; $i++) {
  $j = rand(0, 10);

  if ($j === 2) {
    $a = true;
    continue;
  }

  if ($j === 3) {
    $a = true;
    break;
  }
}
