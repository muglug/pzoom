<?php
$tag = null;
foreach (["a", "b", "c"] as $tag) {
  if ($tag === "a") {
    break;
  } else {
    $tag = null;
  }
}
