<?php
$tag = null;
foreach (["a", "b", "c"] as $tag) {
  if ($tag === "a") {
    $tag = 5;
  } else {
    break;
  }
}
