<?php
function getStrings(): array {
    return ["hello", "world"];
}

$a = null;

foreach (getStrings() as $s) {
  if ($a === null) {
    $a = $s;
  }
}
