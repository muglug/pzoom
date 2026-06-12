<?php
$a = false;

foreach (["a", "b", "c"] as $tag) {
  if ($tag === "a") {
    $a = true;
    break;
  }

  if ($tag === "b") {
    $a = true;
    continue;
  }
}
