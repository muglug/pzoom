<?php
$a = false;
foreach ([1, 2, 3] as $i) {
  if ($i > 1) {
    $a = true;
    continue;
  }

  break;
}
