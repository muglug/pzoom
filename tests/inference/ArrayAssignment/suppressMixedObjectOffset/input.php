<?php
function getThings(): array {
  return [];
}

$arr = [];

foreach (getThings() as $a) {
  $arr[$a->id] = $a;
}

echo $arr[0];
